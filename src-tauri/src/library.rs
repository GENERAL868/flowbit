//! What you've watched: position, duration and watched state for each media file,
//! saved in library.json. mpv reports the position while it plays (see track_playback).

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::Mutex,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

/// Past this share of a video, it counts as watched
const WATCHED_AT: f64 = 0.9;

#[derive(Serialize, Deserialize, Clone, Default, Debug)]
#[serde(default, rename_all = "camelCase")]
pub struct WatchState {
    pub position: f64,
    pub duration: f64,
    pub watched: bool,
    /// Unix time in seconds
    pub last_played: u64,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct LibraryFile {
    items: HashMap<String, WatchState>,
    /// Torrents we created from the user's own files (no Mark of the Web for those)
    shared: HashSet<String>,
    /// Finished torrents the user chose to keep seeding (not paused automatically)
    seeding: HashSet<String>,
}

pub struct Library {
    path: PathBuf,
    data: Mutex<LibraryFile>,
}

pub fn key(info_hash: &str, file: usize) -> String {
    format!("{info_hash}:{file}")
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl Library {
    pub fn load(data_dir: &Path) -> Self {
        let path = data_dir.join("library.json");
        let data = std::fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        Self {
            path,
            data: Mutex::new(data),
        }
    }

    pub fn all(&self) -> HashMap<String, WatchState> {
        self.data.lock().unwrap().items.clone()
    }

    pub fn update(&self, key: &str, f: impl FnOnce(&mut WatchState)) {
        let mut data = self.data.lock().unwrap();
        f(data.items.entry(key.to_owned()).or_default());
        self.save(&data);
    }

    pub fn mark_shared(&self, info_hash: &str) {
        let mut data = self.data.lock().unwrap();
        if data.shared.insert(info_hash.to_owned()) {
            self.save(&data);
        }
    }

    pub fn is_shared(&self, info_hash: &str) -> bool {
        self.data.lock().unwrap().shared.contains(info_hash)
    }

    pub fn set_seeding(&self, info_hash: &str, seeding: bool) {
        let mut data = self.data.lock().unwrap();
        let changed = if seeding {
            data.seeding.insert(info_hash.to_owned())
        } else {
            data.seeding.remove(info_hash)
        };
        if changed {
            self.save(&data);
        }
    }

    pub fn is_seeding(&self, info_hash: &str) -> bool {
        self.data.lock().unwrap().seeding.contains(info_hash)
    }

    /// Forgets torrents that were removed
    pub fn retain(&self, info_hashes: &HashSet<String>) {
        let mut data = self.data.lock().unwrap();
        let count = |d: &LibraryFile| d.items.len() + d.shared.len() + d.seeding.len();
        let before = count(&data);
        data.items
            .retain(|k, _| k.split(':').next().is_some_and(|h| info_hashes.contains(h)));
        data.shared.retain(|h| info_hashes.contains(h));
        data.seeding.retain(|h| info_hashes.contains(h));
        if count(&data) != before {
            self.save(&data);
        }
    }

    fn save(&self, data: &LibraryFile) {
        let tmp = self.path.with_extension("json.tmp");
        if let Ok(bytes) = serde_json::to_vec(data) {
            if std::fs::write(&tmp, bytes).is_ok() {
                std::fs::rename(&tmp, &self.path).ok();
            }
        }
    }
}

/// Where mpv listens for its JSON IPC: a named pipe on Windows, a Unix socket
/// elsewhere. The name has a secret in it, so other programs can't take mpv's place.
pub fn ipc_address(secret: &str) -> String {
    #[cfg(windows)]
    return format!(r"\\.\pipe\flowbit-mpv-{secret}");
    #[cfg(not(windows))]
    return std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|d| d.is_dir())
        .unwrap_or_else(std::env::temp_dir)
        .join(format!("flowbit-mpv-{secret}.sock"))
        .to_string_lossy()
        .into_owned();
}

/// Connects to mpv's IPC, waiting up to 10 s for mpv to open it
#[cfg(windows)]
async fn connect(address: &str) -> Option<tokio::net::windows::named_pipe::NamedPipeClient> {
    use tokio::net::windows::named_pipe::ClientOptions;
    for _ in 0..50 {
        if let Ok(client) = ClientOptions::new().open(address) {
            return Some(client);
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    None
}

#[cfg(unix)]
async fn connect(address: &str) -> Option<tokio::net::UnixStream> {
    for _ in 0..50 {
        if let Ok(stream) = tokio::net::UnixStream::connect(address).await {
            return Some(stream);
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    None
}

/// Follows playback through mpv's JSON IPC until mpv closes, calling
/// `on_update(position, duration, ended)` every few seconds of progress.
pub async fn track_playback(address: String, mut on_update: impl FnMut(f64, f64, bool) + Send) {
    let Some(stream) = connect(&address).await else {
        return;
    };
    let (reader, mut writer) = tokio::io::split(stream);
    for (id, prop) in [(1, "time-pos"), (2, "duration"), (3, "eof-reached")] {
        let cmd = format!("{{\"command\":[\"observe_property\",{id},\"{prop}\"]}}\n");
        if writer.write_all(cmd.as_bytes()).await.is_err() {
            return;
        }
    }
    let mut lines = BufReader::new(reader).lines();
    let (mut position, mut duration, mut ended) = (0.0f64, 0.0f64, false);
    let mut saved_at = -100.0f64;
    while let Ok(Some(line)) = lines.next_line().await {
        let Ok(msg) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        if msg["event"] != "property-change" {
            continue;
        }
        match msg["name"].as_str() {
            Some("time-pos") => position = msg["data"].as_f64().unwrap_or(position),
            Some("duration") => duration = msg["data"].as_f64().unwrap_or(duration),
            Some("eof-reached") => ended = msg["data"].as_bool().unwrap_or(false),
            _ => {}
        }
        if ended || (position - saved_at).abs() >= 5.0 {
            saved_at = position;
            on_update(position, duration, ended);
        }
    }
    on_update(position, duration, ended);
    #[cfg(unix)]
    std::fs::remove_file(&address).ok();
}

/// Applies a playback report: position, and watched once near the end
pub fn apply_progress(state: &mut WatchState, position: f64, duration: f64, ended: bool) {
    state.position = position;
    if duration > 0.0 {
        state.duration = duration;
    }
    state.last_played = now();
    if ended || (state.duration > 0.0 && position / state.duration >= WATCHED_AT) {
        state.watched = true;
    }
}
