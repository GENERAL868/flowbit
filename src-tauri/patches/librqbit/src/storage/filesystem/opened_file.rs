// Modified for Flowbit: see the "Flowbit patch" comment (and PATCHES.md)

use std::{
    fs::File,
    io::IoSlice,
    ops::{Deref, DerefMut},
    path::PathBuf,
};

use anyhow::Context;
use parking_lot::{RwLock, RwLockReadGuard, RwLockWriteGuard};

use crate::Error;

pub trait OurFileExt {
    fn pwrite_all_vectored(&self, offset: u64, bufs: [IoSlice<'_>; 2]) -> anyhow::Result<usize>;
    fn pread_exact(&self, offset: u64, buf: &mut [u8]) -> anyhow::Result<()>;
    fn pwrite_all(&self, offset: u64, buf: &[u8]) -> anyhow::Result<()>;
}

impl OurFileExt for File {
    #[cfg(unix)]
    fn pwrite_all_vectored(&self, offset: u64, bufs: [IoSlice<'_>; 2]) -> anyhow::Result<usize> {
        nix::sys::uio::pwritev(self, &bufs, offset.try_into()?).context("error calling pwritev")
    }

    #[cfg(not(unix))]
    fn pwrite_all_vectored(&self, offset: u64, bufs: [IoSlice<'_>; 2]) -> anyhow::Result<usize> {
        match (bufs[0].len(), bufs[1].len()) {
            (len, 0) if len > 0 => {
                self.pwrite_all(offset, &bufs[0])?;
                Ok(len)
            }
            (0, len) if len > 0 => {
                self.pwrite_all(offset, &bufs[1])?;
                Ok(len)
            }
            (0, 0) => Ok(0),
            (l0, l1) => {
                // concatenate the buffers in memory so that we issue one write call instead of 2
                // assumes the message is <= CHUNK_SIZE
                use librqbit_core::constants::CHUNK_SIZE;
                let mut buf = [0u8; CHUNK_SIZE as usize];

                buf.get_mut(..l0)
                    .context("buf too small")?
                    .copy_from_slice(&bufs[0]);
                buf.get_mut(l0..l0 + l1)
                    .context("buf too small")?
                    .copy_from_slice(&bufs[1]);
                self.pwrite_all(offset, &buf[..l0 + l1])?;
                Ok(l0 + l1)
            }
        }
    }

    #[cfg(unix)]
    fn pread_exact(&self, offset: u64, buf: &mut [u8]) -> anyhow::Result<()> {
        use std::os::unix::fs::FileExt;

        Ok(self.read_exact_at(buf, offset)?)
    }

    #[cfg(windows)]
    fn pread_exact(&self, mut offset: u64, mut buf: &mut [u8]) -> anyhow::Result<()> {
        use std::os::windows::fs::FileExt;
        while !buf.is_empty() {
            let n = self.seek_read(buf, offset)?;
            if n == 0 {
                return Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "eof").into());
            }
            offset += n as u64;
            buf = &mut buf[n..];
        }
        Ok(())
    }

    #[cfg(not(any(windows, unix)))]
    fn pread_exact(&self, offset: u64, buf: &mut [u8]) -> anyhow::Result<()> {
        anyhow::bail!("pread_exact not implemented for your platform")
    }

    #[cfg(unix)]
    fn pwrite_all(&self, offset: u64, buf: &[u8]) -> anyhow::Result<()> {
        use std::os::unix::fs::FileExt;
        Ok(self.write_all_at(buf, offset)?)
    }

    #[cfg(windows)]
    fn pwrite_all(&self, offset: u64, buf: &[u8]) -> anyhow::Result<()> {
        use std::os::windows::fs::FileExt;

        let mut remaining = buf.len();
        let mut buf = buf;
        let mut offset = offset;
        while remaining > 0 {
            let written = self.seek_write(&buf[..remaining], offset)?;
            remaining -= written;
            offset += written as u64;
            buf = &buf[written..];
        }
        Ok(())
    }

    #[cfg(not(any(windows, unix)))]
    fn pwrite_all(&self, offset: u64, buf: &[u8]) -> anyhow::Result<()> {
        anyhow::bail!("pwrite_all not implemented for your platform")
    }
}

#[derive(Default, Debug)]
struct OpenedFileLocked {
    #[allow(unused)]
    path: PathBuf,
    fd: Option<File>,
    #[cfg(windows)]
    tried_marking_sparse: bool,
    /// Flowbit patch: fd was reopened read-only because the file is complete
    read_only: bool,
}

impl Deref for OpenedFileLocked {
    type Target = Option<File>;

    fn deref(&self) -> &Self::Target {
        &self.fd
    }
}

impl DerefMut for OpenedFileLocked {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.fd
    }
}

#[derive(Debug)]
pub(crate) struct OpenedFile {
    file: RwLock<OpenedFileLocked>,
}

impl OpenedFile {
    pub fn new(path: PathBuf, f: File) -> Self {
        Self {
            file: RwLock::new(OpenedFileLocked {
                path,
                fd: Some(f),
                #[cfg(windows)]
                tried_marking_sparse: false,
                read_only: false,
            }),
        }
    }

    /// Flowbit patch: a complete file no longer needs a handle open for writing. On
    /// Windows that handle stops other programs from opening the file with read-only
    /// sharing, so Explorer can't make a thumbnail of a finished video while the
    /// torrent is listed. Reading (playback, seeding) keeps working.
    pub fn reopen_read_only(&self) -> anyhow::Result<()> {
        let mut g = self.file.write();
        if g.read_only || g.fd.is_none() {
            return Ok(());
        }
        let f = std::fs::OpenOptions::new()
            .read(true)
            .open(&g.path)
            .with_context(|| format!("error reopening {:?} read-only", g.path))?;
        g.fd = Some(f);
        g.read_only = true;
        Ok(())
    }

    /// Flowbit patch: opens a read-only file for writing again (e.g. a piece failed
    /// its hash check and is being downloaded again)
    fn ensure_writable(&self) -> anyhow::Result<()> {
        if !self.file.read().read_only {
            return Ok(());
        }
        let mut g = self.file.write();
        if g.read_only && g.fd.is_some() {
            let f = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(&g.path)
                .with_context(|| format!("error reopening {:?} for writing", g.path))?;
            g.fd = Some(f);
            g.read_only = false;
        }
        Ok(())
    }

    /// The file handle, for writing
    pub fn lock_for_write(&self) -> anyhow::Result<impl Deref<Target = File>> {
        self.ensure_writable()?;
        #[cfg(windows)]
        return Ok(self.try_mark_sparse()?);
        #[cfg(not(windows))]
        return Ok(self.lock_read()?);
    }

    pub fn new_dummy() -> Self {
        Self {
            file: RwLock::new(Default::default()),
        }
    }

    pub fn take_clone(&self) -> anyhow::Result<Self> {
        let f = std::mem::take(&mut *self.file.write());
        Ok(Self {
            file: RwLock::new(f),
        })
    }

    pub fn lock_read(&self) -> crate::Result<impl Deref<Target = File>> {
        RwLockReadGuard::try_map(self.file.read(), |f| f.as_ref())
            .ok()
            .ok_or(Error::FsFileIsNone)
    }

    #[allow(dead_code)]
    pub fn lock_write(&self) -> crate::Result<impl DerefMut<Target = File>> {
        RwLockWriteGuard::try_map(self.file.write(), |f| f.as_mut())
            .ok()
            .ok_or(Error::FsFileIsNone)
    }

    #[cfg(windows)]
    pub fn try_mark_sparse(&self) -> crate::Result<impl Deref<Target = File>> {
        {
            let g = self.file.read();
            if g.tried_marking_sparse {
                return RwLockReadGuard::try_map(g, |f| f.fd.as_ref())
                    .ok()
                    .ok_or(Error::FsFileIsNone);
            }
        }
        let mut g = self.file.write();
        if !g.tried_marking_sparse {
            g.tried_marking_sparse = true;
            let f = g.fd.as_ref().ok_or(Error::FsFileIsNone)?;
            // Flowbit patch: this call used to sit inside tracing::debug!(),
            // which skips evaluating its arguments unless debug logging is on, so files
            // were never marked sparse. Windows then reserved each file's full size up
            // front and zero-filled the gap before every write past the end of the data.
            let marked = super::sparse::mark_file_sparse(f);
            tracing::debug!(path=?g.path, marked, "marking sparse");
        }
        let g = parking_lot::RwLockWriteGuard::downgrade(g);
        Ok(RwLockReadGuard::try_map(g, |f| f.fd.as_ref()).ok().unwrap())
    }
}

#[cfg(test)]
mod tests {
    use std::io::Read;

    use librqbit_core::constants::CHUNK_SIZE;
    use peer_binary_protocol::DoubleBufHelper;
    use tempfile::TempDir;

    use crate::storage::filesystem::opened_file::OurFileExt;

    #[test]
    fn test_pwrite_all_vectored() {
        let td = TempDir::with_prefix("test_pwrite_all_vectored").unwrap();
        let mut tmp_buf = [0u8; CHUNK_SIZE as usize];
        for bufsize in [10000usize, CHUNK_SIZE as usize] {
            let mut buf = vec![0u8; bufsize];
            rand::fill(&mut buf[..]);
            for split_point in [0, bufsize / 2, bufsize] {
                let path = td.path().join(format!("file_{bufsize}_{split_point}"));
                let file = std::fs::OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .open(&path)
                    .unwrap();
                let (first, second) = buf.split_at(split_point);
                let bufs = DoubleBufHelper::new(first, second).as_ioslices(bufsize);
                file.pwrite_all_vectored(0, bufs).unwrap();

                let mut file = std::fs::File::open(&path).unwrap();
                assert_eq!(file.metadata().unwrap().len(), bufsize as u64, "{path:?}");
                file.read_exact(&mut tmp_buf[..bufsize]).unwrap();
                assert_eq!(&tmp_buf[..bufsize], buf);
            }
        }
    }
}
