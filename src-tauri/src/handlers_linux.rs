//! Registers the app as the handler for magnet links and .torrent files on Linux, for
//! the current user only: a hidden desktop entry in ~/.local/share/applications, made
//! the default with xdg-mime (part of xdg-utils).

use std::{
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::Context;

const DESKTOP_FILE: &str = "flowbit-handler.desktop";
const MIME_TYPES: &[&str] = &["x-scheme-handler/magnet", "application/x-bittorrent"];

pub fn register(exe: &Path, _file_icon: Option<&Path>) -> anyhow::Result<()> {
    let dir = applications_dir().context("no home folder")?;
    std::fs::create_dir_all(&dir)?;
    let entry = format!(
        "[Desktop Entry]\nType=Application\nName=Flowbit\n\
         Comment=Open magnet links and .torrent files in Flowbit\n\
         Exec={} %u\nTerminal=false\nNoDisplay=true\nMimeType={};\n",
        exec_quote(&launcher(exe)),
        MIME_TYPES.join(";")
    );
    std::fs::write(dir.join(DESKTOP_FILE), entry)?;
    for mime in MIME_TYPES {
        let status = Command::new("xdg-mime")
            .args(["default", DESKTOP_FILE, mime])
            .status()
            .context("xdg-mime isn't installed (it's in the xdg-utils package)")?;
        anyhow::ensure!(status.success(), "xdg-mime couldn't set the default app");
    }
    // Optional: refreshes the cache some desktops read
    Command::new("update-desktop-database")
        .arg(&dir)
        .status()
        .ok();
    Ok(())
}

/// Whether magnet links and .torrent files open with Flowbit
pub fn is_registered(_exe: &Path) -> bool {
    MIME_TYPES.iter().all(|mime| {
        Command::new("xdg-mime")
            .args(["query", "default", mime])
            .output()
            .is_ok_and(|out| String::from_utf8_lossy(&out.stdout).trim() == DESKTOP_FILE)
    })
}

fn applications_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|d| d.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .map(|d| d.join("applications"))
}

/// The program to start: the AppImage file itself when running from one (the
/// executable inside is in a temporary mount that's gone once the app quits)
fn launcher(exe: &Path) -> PathBuf {
    std::env::var_os("APPIMAGE")
        .map(PathBuf::from)
        .filter(|p| p.is_file())
        .unwrap_or_else(|| exe.to_path_buf())
}

/// A path as one quoted argument of a desktop entry's Exec key: `"`, `` ` ``, `$` and
/// `\` get a backslash (and the backslash is escaped again, as the value is a string),
/// `%` is doubled
fn exec_quote(path: &Path) -> String {
    let mut out = String::from("\"");
    for c in path.to_string_lossy().chars() {
        match c {
            '"' | '`' | '$' => {
                out.push('\\');
                out.push(c);
            }
            '\\' => out.push_str("\\\\\\\\"),
            '%' => out.push_str("%%"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exec_quoting() {
        assert_eq!(
            exec_quote(Path::new("/usr/bin/flowbit")),
            "\"/usr/bin/flowbit\""
        );
        assert_eq!(
            exec_quote(Path::new("/home/a b/100%/$x\"y")),
            "\"/home/a b/100%%/\\$x\\\"y\""
        );
    }
}
