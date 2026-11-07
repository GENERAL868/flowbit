use std::{
    collections::HashSet,
    net::Ipv6Addr,
    num::NonZeroU32,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use anyhow::Context;
use librqbit::{
    api::TorrentIdOrHash, dht::DhtPersistenceConfig, limits::LimitsConfig, AddTorrent,
    AddTorrentOptions, AddTorrentResponse, CreateTorrentOptions, DhtSessionConfig, ListenerMode,
    ListenerOptions, ManagedTorrent, Session, SessionOptions, SessionPersistenceConfig,
    TorrentStatsState,
};
use serde::Serialize;

use crate::{prefs::Prefs, stream_server::StreamServer};

/// Port of the local streaming server that mpv plays from. A fixed port, and a
/// secret that's kept between runs, keep stream URLs the same, so mpv remembers
/// where you stopped watching.
const STREAM_PORT: u16 = 52525;

/// Trackers added to new torrents we create
const DEFAULT_TRACKERS: &[&str] = &[
    "udp://tracker.opentrackr.org:1337/announce",
    "udp://open.demonii.com:1337/announce",
    "udp://tracker.torrent.eu.org:451/announce",
];

/// The torrent engine: a librqbit session, plus an HTTP server on localhost that
/// streams files to the player while they download.
pub struct Engine {
    session: Arc<Session>,
    pub stream: Arc<StreamServer>,
    /// Taken by start_streaming(), once the engine is in an Arc
    stream_listener: Mutex<Option<tokio::net::TcpListener>>,
}

impl Engine {
    pub async fn start(data_dir: &Path, prefs: &Prefs) -> anyhow::Result<Self> {
        let download_dir = PathBuf::from(&prefs.download_dir);
        std::fs::create_dir_all(&download_dir).ok();

        let opts = SessionOptions {
            // Remember torrents between runs, and skip rechecking files that were
            // already verified (fast resume)
            persistence: Some(SessionPersistenceConfig::Json {
                folder: Some(data_dir.join("session")),
            }),
            fastresume: true,
            dht: Some(DhtSessionConfig {
                persistence: Some(DhtPersistenceConfig {
                    dump_interval: None,
                    config_filename: Some(data_dir.join("dht.json")),
                }),
                ..Default::default()
            }),
            // TCP only: uTP is still marked experimental in librqbit
            listen: Some(ListenerOptions {
                mode: ListenerMode::TcpOnly,
                listen_addr: (Ipv6Addr::UNSPECIFIED, prefs.listen_port).into(),
                enable_upnp_port_forwarding: true,
                ..Default::default()
            }),
            ratelimits: limits(prefs),
            trackers: prefs
                .trackers
                .iter()
                .filter_map(|t| t.parse().ok())
                .collect(),
            client_name_and_version: Some(format!("Flowbit {}", env!("CARGO_PKG_VERSION"))),
            ..Default::default()
        };

        let session = Session::new_with_opts(download_dir, opts)
            .await
            .context("couldn't start the torrent engine")?;
        let (stream, listener) = StreamServer::bind(STREAM_PORT, stream_secret(data_dir))
            .await
            .context("couldn't start the streaming server")?;

        Ok(Self {
            session,
            stream: Arc::new(stream),
            stream_listener: Mutex::new(Some(listener)),
        })
    }

    pub fn start_streaming(self: &Arc<Self>) {
        if let Some(listener) = self.stream_listener.lock().unwrap().take() {
            crate::stream_server::spawn(listener, self.stream.clone(), Arc::downgrade(self));
        }
    }

    pub fn session(&self) -> &Arc<Session> {
        &self.session
    }

    pub fn handle(&self, id: usize) -> anyhow::Result<Arc<ManagedTorrent>> {
        self.session()
            .get(TorrentIdOrHash::Id(id))
            .context("torrent not found")
    }

    pub fn set_limits(&self, prefs: &Prefs) {
        let limits = limits(prefs);
        let session = self.session();
        session.ratelimits.set_download_bps(limits.download_bps);
        session.ratelimits.set_upload_bps(limits.upload_bps);
    }

    /// Adds a magnet link, info hash, URL or .torrent file path
    pub async fn add(&self, source: &str, output_folder: Option<String>) -> anyhow::Result<usize> {
        let source = source.trim();
        let add = if source.to_ascii_lowercase().starts_with("magnet:") {
            AddTorrent::from_url(strip_loopback_trackers(source))
        } else if is_url_like(source) {
            anyhow::ensure!(
                !url_host(source).is_some_and(is_loopback_host),
                "torrent links to this computer aren't allowed"
            );
            AddTorrent::from_url(source.to_owned())
        } else {
            AddTorrent::from_bytes(read_torrent_file(Path::new(source))?)
        };
        self.add_torrent(add, output_folder).await
    }

    pub async fn add_bytes(&self, bytes: Vec<u8>) -> anyhow::Result<usize> {
        anyhow::ensure!(
            bytes.len() as u64 <= MAX_TORRENT_FILE,
            "not a valid .torrent file"
        );
        self.add_torrent(AddTorrent::from_bytes(bytes), None).await
    }

    async fn add_torrent(
        &self,
        add: AddTorrent<'_>,
        output_folder: Option<String>,
    ) -> anyhow::Result<usize> {
        self.add_with(
            add,
            AddTorrentOptions {
                output_folder,
                // Resume torrents whose files are already there
                overwrite: true,
                ..Default::default()
            },
        )
        .await
    }

    async fn add_with(
        &self,
        add: AddTorrent<'_>,
        opts: AddTorrentOptions,
    ) -> anyhow::Result<usize> {
        let response = self.session().add_torrent(add, Some(opts)).await?;
        match response {
            AddTorrentResponse::Added(id, _) | AddTorrentResponse::AlreadyManaged(id, _) => Ok(id),
            AddTorrentResponse::ListOnly(_) => anyhow::bail!("torrent wasn't added"),
        }
    }

    /// Creates a torrent from a file or folder and starts seeding it. Returns its magnet link.
    pub async fn create(&self, path: &Path) -> anyhow::Result<(usize, String)> {
        let trackers = DEFAULT_TRACKERS.iter().map(|t| t.to_string()).collect();
        let (result, handle) = self
            .session()
            .create_and_serve_torrent(
                path,
                CreateTorrentOptions {
                    name: None,
                    trackers,
                    piece_length: None,
                },
            )
            .await
            .map_err(|err| anyhow::anyhow!("{err}"))?;
        Ok((handle.id(), result.as_magnet().to_string()))
    }

    pub async fn pause(&self, id: usize) -> anyhow::Result<()> {
        let handle = self.handle(id)?;
        if !handle.is_paused() {
            self.session().pause(&handle).await?;
        }
        Ok(())
    }

    pub async fn resume(&self, id: usize) -> anyhow::Result<()> {
        let handle = self.handle(id)?;
        if handle.is_paused() {
            self.session().unpause(&handle).await?;
        }
        Ok(())
    }

    pub async fn delete(&self, id: usize, delete_files: bool) -> anyhow::Result<()> {
        self.session()
            .delete(TorrentIdOrHash::Id(id), delete_files)
            .await
    }

    /// With no files chosen there's nothing to download or share, so the torrent
    /// pauses; choosing a file again resumes it. Files on disk are kept either way.
    pub async fn set_files(&self, id: usize, files: Vec<usize>) -> anyhow::Result<()> {
        let handle = self.handle(id)?;
        let was_empty = handle.only_files().is_some_and(|o| o.is_empty());
        let files: HashSet<usize> = files.into_iter().collect();
        self.session().update_only_files(&handle, &files).await?;
        if files.is_empty() {
            self.pause(id).await
        } else if was_empty {
            self.resume(id).await
        } else {
            Ok(())
        }
    }

    /// Makes sure a file is being downloaded, so it can be streamed
    pub async fn include_file(&self, id: usize, file: usize) -> anyhow::Result<()> {
        let handle = self.handle(id)?;
        if let Some(mut only) = handle.only_files() {
            if !only.contains(&file) {
                only.push(file);
                self.set_files(id, only).await?;
            }
        }
        self.resume(id).await
    }

    pub fn list(&self) -> Vec<TorrentView> {
        self.session().with_torrents(|torrents| {
            torrents
                .map(|(id, handle)| TorrentView::new(id, handle))
                .collect()
        })
    }

    pub fn files(&self, id: usize) -> anyhow::Result<Vec<FileView>> {
        let handle = self.handle(id)?;
        let only = handle.only_files();
        let stats = handle.stats();
        // Names are the paths the files are saved to: made safe for Windows, see
        // patches/librqbit/src/safe_path.rs
        handle.with_metadata(|meta| {
            meta.file_infos
                .iter()
                .enumerate()
                .map(|(index, fi)| FileView {
                    index,
                    name: fi.relative_filename.to_string_lossy().into_owned(),
                    length: fi.len,
                    progress: stats.file_progress.get(index).copied().unwrap_or(0),
                    included: only.as_ref().is_none_or(|o| o.contains(&index)),
                    padding: fi.attrs.padding,
                })
                .collect()
        })
    }

    /// Where a file is saved, always inside the torrent's folder
    pub fn file_path(&self, id: usize, file: usize) -> anyhow::Result<PathBuf> {
        let handle = self.handle(id)?;
        let relative = handle.with_metadata(|meta| {
            meta.file_infos
                .get(file)
                .map(|fi| fi.relative_filename.clone())
        })?;
        let relative = relative.context("file not found")?;
        librqbit::safe_path::ensure_relative(&relative)?;
        Ok(handle.output_folder().join(relative))
    }

    pub fn torrent_bytes(&self, id: usize) -> anyhow::Result<Vec<u8>> {
        self.handle(id)?
            .with_metadata(|meta| meta.torrent_bytes.to_vec())
    }

    pub fn stream_url(&self, id: usize, file: usize, name: &str) -> String {
        self.stream.url(id, file, name)
    }

    pub fn listen_port(&self) -> Option<u16> {
        self.session().announce_port()
    }
}

fn limits(prefs: &Prefs) -> LimitsConfig {
    LimitsConfig {
        download_bps: NonZeroU32::new(prefs.download_limit.saturating_mul(1024)),
        upload_bps: NonZeroU32::new(prefs.upload_limit.saturating_mul(1024)),
    }
}

/// Largest .torrent file we read (big torrents with many files are a few MB)
const MAX_TORRENT_FILE: u64 = 20 * 1024 * 1024;

/// Reads a local .torrent file, and nothing else
pub fn read_torrent_file(path: &Path) -> anyhow::Result<Vec<u8>> {
    let is_torrent = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("torrent"));
    anyhow::ensure!(is_torrent, "not a magnet link, URL or .torrent file");
    let meta =
        std::fs::metadata(path).with_context(|| format!("couldn't read {}", path.display()))?;
    anyhow::ensure!(
        meta.is_file() && meta.len() <= MAX_TORRENT_FILE,
        "not a valid .torrent file"
    );
    Ok(std::fs::read(path)?)
}

/// The streaming server's secret, kept in the data folder so stream URLs stay the same
fn stream_secret(data_dir: &Path) -> String {
    let path = data_dir.join("stream-secret");
    if let Ok(secret) = std::fs::read_to_string(&path) {
        let secret = secret.trim();
        if secret.len() == 32 && secret.chars().all(|c| c.is_ascii_hexdigit()) {
            return secret.to_owned();
        }
    }
    let secret = crate::stream_server::random_secret();
    std::fs::write(&path, &secret).ok();
    secret
}

/// Removes trackers on this computer from a magnet link. A magnet link opened from a
/// website could otherwise make us send requests to local services (SSRF).
fn strip_loopback_trackers(magnet: &str) -> String {
    let Some((base, query)) = magnet.split_once('?') else {
        return magnet.to_owned();
    };
    let kept: Vec<&str> = query
        .split('&')
        .filter(|param| match param.split_once('=') {
            Some((key, value))
                if key.eq_ignore_ascii_case("tr")
                    || key.to_ascii_lowercase().starts_with("tr.") =>
            {
                !url_host(&percent_decode(value)).is_some_and(is_loopback_host)
            }
            _ => true,
        })
        .collect();
    format!("{base}?{}", kept.join("&"))
}

/// The host of a URL like "scheme://host:port/path"
fn url_host(url: &str) -> Option<String> {
    let rest = url.split_once("://")?.1;
    let authority = rest.split(['/', '?', '#']).next()?;
    let host_port = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    let host = if let Some(v6) = host_port.strip_prefix('[') {
        v6.split(']').next()?
    } else {
        host_port.split(':').next()?
    };
    Some(host.to_ascii_lowercase())
}

fn is_loopback_host(host: String) -> bool {
    let host = host.trim_end_matches('.');
    if host == "localhost" || host.ends_with(".localhost") {
        return true;
    }
    match host.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V4(ip)) => ip.is_loopback() || ip.is_unspecified(),
        Ok(std::net::IpAddr::V6(ip)) => {
            ip.is_loopback()
                || ip.is_unspecified()
                || ip.to_ipv4_mapped().is_some_and(|v4| v4.is_loopback())
        }
        // Decimal or hex forms of 127.0.0.1 ("2130706433", "0x7f000001")
        Err(_) => {
            let n = if let Some(hex) = host.strip_prefix("0x") {
                u32::from_str_radix(hex, 16).ok()
            } else {
                host.parse::<u32>().ok()
            };
            n.is_some_and(|n| n >> 24 == 127 || n == 0)
        }
    }
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    let hex = |b: u8| (b as char).to_digit(16).map(|d| d as u8);
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(hi), Some(lo)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                out.push(hi << 4 | lo);
                i += 3;
                continue;
            }
        }
        out.push(if bytes[i] == b'+' { b' ' } else { bytes[i] });
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_trackers_are_removed() {
        let m = "magnet:?xt=urn:btih:abc&tr=http%3A%2F%2F127.0.0.1%3A8080%2Fannounce&tr=udp%3A%2F%2Ftracker.example.org%3A1337&tr=http://localhost/x&dn=a";
        assert_eq!(
            strip_loopback_trackers(m),
            "magnet:?xt=urn:btih:abc&tr=udp%3A%2F%2Ftracker.example.org%3A1337&dn=a"
        );
        assert!(url_host("http://[::1]:80/").is_some_and(is_loopback_host));
        assert!(url_host("http://2130706433/").is_some_and(is_loopback_host));
        assert!(url_host("http://user@127.1.2.3/").is_some_and(is_loopback_host));
        assert!(!url_host("https://example.org/a.torrent").is_some_and(is_loopback_host));
        assert!(!url_host("http://192.168.1.5:8080/a").is_some_and(is_loopback_host));
    }

    #[test]
    fn torrent_paths_stay_in_the_download_folder() {
        use librqbit::safe_path::{ensure_relative, safe_relative_path, sanitize_component};
        assert_eq!(sanitize_component("C:evil.exe"), "C_evil.exe");
        assert_eq!(sanitize_component("file.txt:stream"), "file.txt_stream");
        assert_eq!(sanitize_component("CON"), "_CON");
        assert_eq!(sanitize_component("com1.txt"), "_com1.txt");
        assert_eq!(sanitize_component("name. . "), "name");
        assert_eq!(sanitize_component(".."), "_");
        assert_eq!(
            sanitize_component("Movie (2020) [1080p].mkv"),
            "Movie (2020) [1080p].mkv"
        );
        let p =
            safe_relative_path(["C:", "Windows", "evil.dll"].into_iter().map(Into::into)).unwrap();
        assert_eq!(p, PathBuf::from("C_").join("Windows").join("evil.dll"));
        let base = PathBuf::from(r"D:\Downloads\Show");
        assert!(base.join(&p).starts_with(&base));
        assert!(ensure_relative(Path::new("C:x")).is_err());
        assert!(ensure_relative(Path::new("..")).is_err());
        assert!(ensure_relative(Path::new(r"\\server\share")).is_err());
    }
}

pub fn is_url_like(s: &str) -> bool {
    let lower = s.to_ascii_lowercase();
    lower.starts_with("magnet:")
        || lower.starts_with("http://")
        || lower.starts_with("https://")
        || (s.len() == 40 && s.chars().all(|c| c.is_ascii_hexdigit()))
        || (s.len() == 32 && s.chars().all(|c| c.is_ascii_alphanumeric()) && !s.contains('.'))
}

/// What the torrent list shows for each torrent
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TorrentView {
    pub id: usize,
    pub info_hash: String,
    pub name: Option<String>,
    pub output_folder: String,
    /// "initializing", "live", "paused" or "error"
    pub state: &'static str,
    pub error: Option<String>,
    pub finished: bool,
    pub progress_bytes: u64,
    pub total_bytes: u64,
    pub uploaded_bytes: u64,
    pub down_speed: u64,
    pub up_speed: u64,
    pub eta_secs: Option<u64>,
    pub peers: u32,
    pub file_count: usize,
    pub has_metadata: bool,
    /// Filled in by the app (posters live in its data folder)
    pub has_poster: bool,
}

impl TorrentView {
    fn new(id: usize, handle: &Arc<ManagedTorrent>) -> Self {
        let stats = handle.stats();
        let state = match stats.state {
            TorrentStatsState::Initializing { .. } => "initializing",
            TorrentStatsState::Live => "live",
            TorrentStatsState::Paused => "paused",
            TorrentStatsState::Error => "error",
        };
        let (down_speed, up_speed, eta_secs, peers) = match &stats.live {
            Some(live) => (
                live.download_speed.as_bytes(),
                live.upload_speed.as_bytes(),
                eta(
                    stats.total_bytes.saturating_sub(stats.progress_bytes),
                    live.download_speed.as_bytes(),
                ),
                live.snapshot.peer_stats.live,
            ),
            None => (0, 0, None, 0),
        };
        let metadata = handle.metadata.load();
        // librqbit counts only the chosen files: with none chosen it would say
        // finished with nothing to show, so count every file instead
        let (mut finished, mut progress_bytes, mut total_bytes) =
            (stats.finished, stats.progress_bytes, stats.total_bytes);
        if let (Some(meta), true) = (
            metadata.as_ref(),
            handle.only_files().is_some_and(|o| o.is_empty()),
        ) {
            total_bytes = meta
                .file_infos
                .iter()
                .filter(|fi| !fi.attrs.padding)
                .map(|fi| fi.len)
                .sum();
            progress_bytes = meta
                .file_infos
                .iter()
                .zip(&stats.file_progress)
                .filter(|(fi, _)| !fi.attrs.padding)
                .map(|(fi, p)| (*p).min(fi.len))
                .sum();
            finished = progress_bytes >= total_bytes;
        }
        Self {
            id,
            info_hash: handle.info_hash().as_string(),
            name: handle.name(),
            output_folder: handle.output_folder().to_string_lossy().into_owned(),
            state,
            error: stats.error.clone(),
            finished,
            progress_bytes,
            total_bytes,
            uploaded_bytes: stats.uploaded_bytes,
            down_speed,
            up_speed,
            eta_secs,
            peers,
            file_count: metadata
                .as_ref()
                .map(|m| {
                    m.info
                        .iter_file_details()
                        .filter(|f| !f.attrs().padding)
                        .count()
                })
                .unwrap_or(0),
            has_metadata: metadata.is_some(),
            has_poster: false,
        }
    }
}

fn eta(remaining: u64, speed: u64) -> Option<u64> {
    (speed > 0 && remaining > 0).then(|| remaining / speed)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileView {
    pub index: usize,
    pub name: String,
    pub length: u64,
    pub progress: u64,
    pub included: bool,
    pub padding: bool,
}
