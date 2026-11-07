//! Flowbit patch (new file, not part of librqbit 9.0.1): safe local paths from torrent file names.
//!
//! Torrent metadata only rejected ".." and path separators inside a name. On Windows
//! that isn't enough: a component like "C:evil.exe" makes `PathBuf::push` replace the
//! whole path, so a torrent could write (or, when removed with its files, delete) a
//! file outside the download folder. Names like "file:stream" write to an alternate
//! data stream, and "CON" or "COM1" open devices.

use std::path::{Component, Path, PathBuf};

const RESERVED: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM0", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
    "COM8", "COM9", "LPT0", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    "CONIN$", "CONOUT$",
];

/// Makes one path component safe to use as a file or folder name on any platform
pub fn sanitize_component(name: &str) -> String {
    let mut out: String = name
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if (c as u32) < 32 => '_',
            c => c,
        })
        .collect();
    // Windows drops trailing dots and spaces, so "a." and "a" would be the same file
    while out.ends_with('.') || out.ends_with(' ') {
        out.pop();
    }
    let stem = out.split('.').next().unwrap_or("").to_ascii_uppercase();
    if out.is_empty() || out == "." || out == ".." {
        out = "_".to_owned();
    } else if RESERVED.contains(&stem.trim_end()) {
        out.insert(0, '_');
    }
    out
}

/// Builds a relative path from a torrent's name components, safe to join to the
/// download folder
pub fn safe_relative_path<'a>(components: impl Iterator<Item = std::borrow::Cow<'a, str>>) -> anyhow::Result<PathBuf> {
    let mut path = PathBuf::new();
    for c in components {
        path.push(sanitize_component(&c));
    }
    ensure_relative(&path)?;
    Ok(path)
}

/// Fails unless the path is made of plain names only (no drive, root, "." or "..")
pub fn ensure_relative(path: &Path) -> anyhow::Result<()> {
    if path.as_os_str().is_empty() || path.components().any(|c| !matches!(c, Component::Normal(_))) {
        anyhow::bail!("unsafe path in torrent: {path:?}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize() {
        assert_eq!(sanitize_component("C:evil.exe"), "C_evil.exe");
        assert_eq!(sanitize_component("file.txt:stream"), "file.txt_stream");
        assert_eq!(sanitize_component("CON"), "_CON");
        assert_eq!(sanitize_component("com1.txt"), "_com1.txt");
        assert_eq!(sanitize_component("name. . "), "name");
        assert_eq!(sanitize_component(".."), "_");
        assert_eq!(sanitize_component("Movie (2020).mkv"), "Movie (2020).mkv");
    }

    #[test]
    fn test_safe_relative_path() {
        let p = safe_relative_path(["C:", "Windows", "evil.dll"].into_iter().map(Into::into)).unwrap();
        assert_eq!(p, PathBuf::from("C_").join("Windows").join("evil.dll"));
        assert!(ensure_relative(Path::new("C:x")).is_err());
        assert!(ensure_relative(Path::new("..")).is_err());
    }
}
