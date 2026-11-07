//! Torrent posters: a frame from the main video (or an audio file's cover art),
//! grabbed by the bundled mpv without opening a window, saved as posters/<hash>.jpg.

use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::Context;

pub fn path(data_dir: &Path, info_hash: &str) -> PathBuf {
    data_dir.join("posters").join(format!("{info_hash}.jpg"))
}

/// Grabs one frame, 15% into the stream (past opening logos), at most 480 px wide
pub async fn generate(mpv: &Path, url: &str, out: &Path) -> anyhow::Result<()> {
    let work = out.with_extension("work");
    std::fs::create_dir_all(&work)?;
    let mut cmd = tokio::process::Command::new(mpv);
    cmd.arg("--no-config")
        .arg("--msg-level=all=no")
        .arg("--load-scripts=no")
        .arg("--ytdl=no")
        .arg("--audio=no")
        .arg("--sub=no")
        .arg("--vo=image")
        .arg("--vo-image-format=jpg")
        .arg("--vo-image-jpeg-quality=85")
        .arg(format!("--vo-image-outdir={}", work.display()))
        .arg("--vf=scale=480:-2")
        .arg("--frames=1")
        .arg("--start=15%")
        .arg("--hr-seek=no")
        .arg("--network-timeout=60")
        .arg("--")
        .arg(url)
        .kill_on_drop(true);
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW

    // mpv can linger after writing the frame (closing the stream), so watch for the
    // image instead of waiting for it to exit
    let find_frame = || {
        std::fs::read_dir(&work)
            .ok()?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .find(|p| p.extension().is_some_and(|e| e == "jpg"))
    };
    let result = async {
        let mut child = cmd.spawn()?;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(120);
        let frame = loop {
            if let Some(frame) = find_frame() {
                // Let mpv finish writing it
                tokio::time::sleep(Duration::from_millis(500)).await;
                break frame;
            }
            if let Ok(Some(_)) = child.try_wait() {
                break find_frame().context("no frame")?;
            }
            anyhow::ensure!(tokio::time::Instant::now() < deadline, "took too long");
            tokio::time::sleep(Duration::from_millis(300)).await;
        };
        child.kill().await.ok();
        std::fs::create_dir_all(out.parent().context("no folder")?)?;
        std::fs::rename(frame, out)?;
        Ok(())
    }
    .await;
    std::fs::remove_dir_all(&work).ok();
    result
}

/// The poster as a data: URL for the window
pub fn data_url(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    Some(format!("data:image/jpeg;base64,{}", base64(&bytes)))
}

fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (chunk[0] as u32) << 16
            | (*chunk.get(1).unwrap_or(&0) as u32) << 8
            | *chunk.get(2).unwrap_or(&0) as u32;
        out.push(TABLE[(n >> 18) as usize & 63] as char);
        out.push(TABLE[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            TABLE[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// Marks a downloaded file as coming from the internet ("Mark of the Web"), like
/// browsers do, so Windows SmartScreen and Office warn before it runs or opens
/// with macros. Skipped if the file already has a mark.
pub fn mark_of_the_web(file: &Path) {
    #[cfg(windows)]
    {
        if !file.is_file() {
            return;
        }
        let mut ads = file.as_os_str().to_owned();
        ads.push(":Zone.Identifier");
        let ads = PathBuf::from(ads);
        if ads.exists() {
            return;
        }
        std::fs::write(ads, "[ZoneTransfer]\r\nZoneId=3\r\n").ok();
    }
    #[cfg(not(windows))]
    let _ = file;
}

#[cfg(test)]
mod tests {
    #[test]
    fn base64() {
        assert_eq!(super::base64(b"Man"), "TWFu");
        assert_eq!(super::base64(b"Ma"), "TWE=");
        assert_eq!(super::base64(b"M"), "TQ==");
    }
}
