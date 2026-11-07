use std::{
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::Context;

use crate::prefs::Prefs;

/// Media that mpv plays. Anything else opens in the system's default app.
const MEDIA_EXTENSIONS: &[&str] = &[
    // video
    "mkv", "mp4", "m4v", "webm", "avi", "mov", "wmv", "flv", "ts", "m2ts", "mts", "mpg", "mpeg",
    "ogv", "3gp", "vob", "rmvb", "hevc", // audio
    "mp3", "flac", "m4a", "aac", "ogg", "opus", "wav", "wma", "alac", "ape", "mka", "ac3", "dts",
];

const AUDIO_EXTENSIONS: &[&str] = &[
    "mp3", "flac", "m4a", "aac", "ogg", "opus", "wav", "wma", "alac", "ape", "mka", "ac3", "dts",
];

pub fn is_video(name: &str) -> bool {
    is_media(name)
        && !Path::new(name)
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| AUDIO_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

pub fn is_media(name: &str) -> bool {
    Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| MEDIA_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

/// Plays a stream URL in the bundled mpv, in its own window with mpv's own controls,
/// shortcuts and subtitle rendering.
pub fn play(
    mpv: &Path,
    data_dir: &Path,
    url: &str,
    title: &str,
    prefs: &Prefs,
    ipc_pipe: &str,
) -> anyhow::Result<()> {
    let watch_later = data_dir.join("watch_later");
    std::fs::create_dir_all(&watch_later).ok();

    let mut cmd = Command::new(mpv);
    cmd.arg(format!("--force-media-title={title}"))
        .arg(format!("--watch-later-dir={}", watch_later.display()))
        .arg("--save-position-on-quit=yes")
        .arg("--force-window=immediate")
        .arg("--keep-open=yes")
        .arg("--ytdl=no")
        // Only for this file: don't follow playlists or links inside downloaded files
        .arg("--load-unsafe-playlists=no")
        .arg("--access-references=no")
        // Reports the position back, for the watch library
        .arg(format!("--input-ipc-server={ipc_pipe}"));
    if !prefs.subtitle_languages.trim().is_empty() {
        cmd.arg(format!("--slang={}", prefs.subtitle_languages.trim()));
    }
    if !prefs.audio_languages.trim().is_empty() {
        cmd.arg(format!("--alang={}", prefs.audio_languages.trim()));
    }
    // The bundled mpv on Windows reads these from portable_config/mpv.conf. The
    // system's mpv elsewhere keeps the user's own config, plus what streaming needs:
    // buffer ahead, and wait for slow peers instead of giving up.
    #[cfg(not(windows))]
    cmd.arg("--cache=yes")
        .arg("--demuxer-max-bytes=200MiB")
        .arg("--demuxer-readahead-secs=60")
        .arg("--network-timeout=120");
    cmd.arg("--").arg(url);

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // mpv.exe is a GUI app, but don't let it open a console window either way
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    cmd.spawn().context("couldn't start the player")?;
    Ok(())
}

#[cfg(windows)]
const MPV: &str = "mpv.exe";
#[cfg(not(windows))]
const MPV: &str = "mpv";

/// Bundled mpv (Windows), or the one next to the executable in development, else
/// mpv from the PATH (Linux packages depend on the system's mpv)
pub fn mpv_path(resource_dir: Option<PathBuf>) -> PathBuf {
    let candidates = [
        resource_dir.map(|d| d.join("resources").join("mpv").join(MPV)),
        std::env::current_exe().ok().and_then(|exe| {
            exe.parent()
                .map(|d| d.join("resources").join("mpv").join(MPV))
        }),
        Some(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("resources")
                .join("mpv")
                .join(MPV),
        ),
    ];
    candidates
        .into_iter()
        .flatten()
        .find(|p| p.is_file())
        .unwrap_or_else(|| PathBuf::from(MPV))
}
