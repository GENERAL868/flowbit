#[cfg(windows)]
mod drop;
mod engine;
#[cfg(windows)]
mod handlers;
#[cfg(target_os = "linux")]
#[path = "handlers_linux.rs"]
mod handlers;
mod i18n;
mod keys;
mod library;
mod media_cmds;
mod player;
mod posters;
mod prefs;
mod stream_server;
mod subtitles;
mod translate;

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::{atomic::AtomicBool, Arc, Mutex},
    time::Duration,
};

use serde::Serialize;
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, RunEvent, State, WindowEvent,
};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_opener::OpenerExt;

use engine::{Engine, FileView, TorrentView};
use prefs::Prefs;

type CmdResult<T> = Result<T, String>;

struct AppState {
    data_dir: PathBuf,
    mpv: PathBuf,
    prefs: Mutex<Prefs>,
    /// Started in the background at launch; commands wait for it
    engine: tokio::sync::OnceCell<Result<Arc<Engine>, String>>,
    library: library::Library,
    keys: keys::KeyStore,
    translate_cancel: AtomicBool,
}

impl AppState {
    async fn engine(&self) -> CmdResult<Arc<Engine>> {
        self.engine
            .get_or_init(|| async {
                let prefs = self.prefs();
                let engine = Engine::start(&self.data_dir, &prefs)
                    .await
                    .map(Arc::new)
                    .map_err(|err| format!("{err:#}"))?;
                engine.start_streaming();
                Ok(engine)
            })
            .await
            .clone()
    }

    fn prefs(&self) -> Prefs {
        self.prefs.lock().unwrap().clone()
    }

    fn lang(&self) -> &'static str {
        i18n::language(&self.prefs.lock().unwrap().language)
    }
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// mpv missing (on Linux it comes from the system) gets a message saying how to
/// install it
fn player_error(state: &AppState, e: anyhow::Error) -> String {
    let missing = e.chain().any(|c| {
        c.downcast_ref::<std::io::Error>()
            .is_some_and(|io| io.kind() == std::io::ErrorKind::NotFound)
    });
    if missing {
        i18n::t(state.lang(), "err.noMpv").to_string()
    } else {
        err(e)
    }
}

// ---------------------------------------------------------------------------
// Commands for the window

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AppInfo {
    version: &'static str,
    /// "windows", "linux"…, for the few texts that differ
    platform: &'static str,
    prefs: Prefs,
    is_default_app: bool,
    start_on_login: bool,
}

#[tauri::command]
fn app_info(app: AppHandle, state: State<'_, AppState>) -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION"),
        platform: std::env::consts::OS,
        prefs: state.prefs(),
        is_default_app: is_default_app(),
        start_on_login: app.autolaunch().is_enabled().unwrap_or(false),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Snapshot {
    torrents: Vec<TorrentView>,
    down_speed: u64,
    up_speed: u64,
    listen_port: Option<u16>,
    disk_free: Option<u64>,
    disk_total: Option<u64>,
}

#[tauri::command]
async fn snapshot(state: State<'_, AppState>) -> CmdResult<Snapshot> {
    let engine = state.engine().await?;
    let mut torrents = engine.list();
    for t in &mut torrents {
        t.has_poster = posters::path(&state.data_dir, &t.info_hash).exists();
    }
    let down_speed = torrents.iter().map(|t| t.down_speed).sum();
    let up_speed = torrents.iter().map(|t| t.up_speed).sum();
    let disk = disk_space(Path::new(&state.prefs().download_dir));
    Ok(Snapshot {
        torrents,
        down_speed,
        up_speed,
        listen_port: engine.listen_port(),
        disk_free: disk.map(|d| d.0),
        disk_total: disk.map(|d| d.1),
    })
}

#[tauri::command]
async fn files(state: State<'_, AppState>, id: usize) -> CmdResult<Vec<FileView>> {
    state.engine().await?.files(id).map_err(err)
}

#[tauri::command]
async fn add(state: State<'_, AppState>, source: String) -> CmdResult<usize> {
    state.engine().await?.add(&source, None).await.map_err(err)
}

/// Dropped or chosen paths: .torrent files are added, anything else is shared
#[tauri::command]
async fn add_paths(state: State<'_, AppState>, paths: Vec<String>) -> CmdResult<Vec<usize>> {
    let engine = state.engine().await?;
    let mut ids = Vec::new();
    for path in paths {
        let id = if path.to_lowercase().ends_with(".torrent") {
            engine.add(&path, None).await.map_err(err)?
        } else {
            let id = engine.create(Path::new(&path)).await.map_err(err)?.0;
            mark_shared(&state, &engine, id);
            id
        };
        ids.push(id);
    }
    Ok(ids)
}

#[tauri::command]
async fn add_bytes(state: State<'_, AppState>, bytes: Vec<u8>) -> CmdResult<usize> {
    state.engine().await?.add_bytes(bytes).await.map_err(err)
}

#[tauri::command]
async fn choose_torrent_file(
    app: AppHandle,
    state: State<'_, AppState>,
) -> CmdResult<Option<usize>> {
    let lang = state.lang();
    let picked = app
        .dialog()
        .file()
        .set_title(i18n::t(lang, "dialog.openTorrent"))
        .add_filter(i18n::t(lang, "filter.torrent"), &["torrent"])
        .blocking_pick_file();
    let Some(path) = picked.and_then(|p| p.into_path().ok()) else {
        return Ok(None);
    };
    let engine = state.engine().await?;
    engine
        .add(&path.to_string_lossy(), None)
        .await
        .map(Some)
        .map_err(err)
}

/// Shares a folder or a file: creates a torrent and seeds it from where it is
#[tauri::command]
async fn create_torrent(
    app: AppHandle,
    state: State<'_, AppState>,
    folder: bool,
) -> CmdResult<Option<String>> {
    let lang = state.lang();
    let dialog = app.dialog().file().set_title(i18n::t(
        lang,
        if folder {
            "dialog.shareFolder"
        } else {
            "dialog.shareFile"
        },
    ));
    let picked = if folder {
        dialog.blocking_pick_folder()
    } else {
        dialog.blocking_pick_file()
    };
    let Some(path) = picked.and_then(|p| p.into_path().ok()) else {
        return Ok(None);
    };
    let engine = state.engine().await?;
    let (id, magnet) = engine.create(&path).await.map_err(err)?;
    mark_shared(&state, &engine, id);
    Ok(Some(magnet))
}

/// Remembers torrents made from the user's own files: those get no Mark of the Web
fn mark_shared(state: &AppState, engine: &Engine, id: usize) {
    if let Ok(handle) = engine.handle(id) {
        state.library.mark_shared(&handle.info_hash().as_string());
    }
}

/// Whether a finished torrent gets paused instead of seeding: not the user's own
/// shares, and not ones they started seeding themselves
fn stops_when_done(state: &AppState, prefs: &Prefs, t: &TorrentView) -> bool {
    prefs.stop_seeding_when_done
        && t.finished
        && t.has_metadata
        && !state.library.is_shared(&t.info_hash)
        && !state.library.is_seeding(&t.info_hash)
}

#[tauri::command]
async fn pause(state: State<'_, AppState>, id: usize) -> CmdResult<()> {
    let engine = state.engine().await?;
    if let Ok(handle) = engine.handle(id) {
        state
            .library
            .set_seeding(&handle.info_hash().as_string(), false);
    }
    engine.pause(id).await.map_err(err)
}

#[tauri::command]
async fn resume(state: State<'_, AppState>, id: usize) -> CmdResult<()> {
    let engine = state.engine().await?;
    // Resuming a finished torrent means "start seeding": keep it that way
    if let Some(t) = engine.list().into_iter().find(|t| t.id == id && t.finished) {
        state.library.set_seeding(&t.info_hash, true);
    }
    engine.resume(id).await.map_err(err)
}

#[tauri::command]
async fn pause_all(state: State<'_, AppState>) -> CmdResult<()> {
    let engine = state.engine().await?;
    for t in engine.list() {
        engine.pause(t.id).await.ok();
    }
    Ok(())
}

#[tauri::command]
async fn resume_all(state: State<'_, AppState>) -> CmdResult<()> {
    let engine = state.engine().await?;
    resume_unfinished(&state, &engine).await;
    Ok(())
}

/// "Resume all": downloads continue, finished torrents stay stopped if they would
/// be stopped anyway
async fn resume_unfinished(state: &AppState, engine: &Engine) {
    let prefs = state.prefs();
    for t in engine.list() {
        if !stops_when_done(state, &prefs, &t) {
            engine.resume(t.id).await.ok();
        }
    }
}

#[tauri::command]
async fn remove(state: State<'_, AppState>, id: usize, delete_files: bool) -> CmdResult<()> {
    state
        .engine()
        .await?
        .delete(id, delete_files)
        .await
        .map_err(err)
}

#[tauri::command]
async fn set_files(state: State<'_, AppState>, id: usize, files: Vec<usize>) -> CmdResult<()> {
    state
        .engine()
        .await?
        .set_files(id, files)
        .await
        .map_err(err)
}

/// Streams a file in mpv. Without a file index, plays the torrent's largest media
/// file. While it plays, the watch library follows the position.
#[tauri::command]
async fn play(
    app: AppHandle,
    state: State<'_, AppState>,
    id: usize,
    file: Option<usize>,
) -> CmdResult<()> {
    let engine = state.engine().await?;
    let files = engine.files(id).map_err(err)?;
    let file = match file {
        Some(index) => files.into_iter().find(|f| f.index == index),
        None => files
            .into_iter()
            .filter(|f| !f.padding && player::is_media(&f.name))
            .max_by_key(|f| f.length),
    }
    .ok_or("Nothing to play in this torrent")?;

    engine.include_file(id, file.index).await.map_err(err)?;
    let url = engine.stream_url(id, file.index, &file.name);
    let title = Path::new(&file.name)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or(file.name.clone());
    let pipe = library::ipc_address(&stream_server::random_secret());
    player::play(
        &state.mpv,
        &state.data_dir,
        &url,
        &title,
        &state.prefs(),
        &pipe,
    )
    .map_err(|e| player_error(&state, e))?;

    let key = library::key(
        &engine.handle(id).map_err(err)?.info_hash().as_string(),
        file.index,
    );
    state
        .library
        .update(&key, |s| s.last_played = library::now());
    tauri::async_runtime::spawn(async move {
        library::track_playback(pipe, |position, duration, ended| {
            app.state::<AppState>().library.update(&key, |s| {
                library::apply_progress(s, position, duration, ended)
            });
            app.emit("library-changed", ()).ok();
        })
        .await;
    });
    Ok(())
}

/// Files that run code when opened. Downloaded ones are shown in their folder
/// instead, so one click on a file from a torrent can't run a program.
const RUNNABLE_EXTENSIONS: &[&str] = &[
    "exe",
    "com",
    "scr",
    "pif",
    "msi",
    "msp",
    "bat",
    "cmd",
    "ps1",
    "psm1",
    "vbs",
    "vbe",
    "js",
    "jse",
    "wsf",
    "wsh",
    "hta",
    "lnk",
    "url",
    "reg",
    "cpl",
    "dll",
    "jar",
    "msc",
    "appref-ms",
    "application",
    "gadget",
    "inf",
    "scf",
    "settingcontent-ms",
    "library-ms",
    "search-ms",
    "diagcab",
    "chm",
    "xll",
    "website",
    "appx",
    "msix",
    "appinstaller",
    "iso",
    "img",
    "vhd",
    "vhdx",
    // Linux: launchers, scripts, packages and programs
    "desktop",
    "sh",
    "bash",
    "zsh",
    "run",
    "bin",
    "appimage",
    "py",
    "pl",
    "rb",
    "deb",
    "rpm",
    "flatpakref",
    "so",
];

fn is_runnable(path: &Path) -> bool {
    // On Linux, any file marked executable can run
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if std::fs::metadata(path).is_ok_and(|m| m.permissions().mode() & 0o111 != 0) {
            return true;
        }
    }
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| RUNNABLE_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(true)
}

/// Opens a downloaded file in its default app, or plays media in mpv. Programs and
/// scripts are shown in their folder instead (see RUNNABLE_EXTENSIONS).
#[tauri::command]
async fn open_file(
    app: AppHandle,
    state: State<'_, AppState>,
    id: usize,
    file: usize,
) -> CmdResult<&'static str> {
    let engine = state.engine().await?;
    let path = engine.file_path(id, file).map_err(err)?;
    if player::is_media(&path.to_string_lossy()) {
        drop(engine);
        return play(app, state, id, Some(file)).await.map(|_| "played");
    }
    if is_runnable(&path) {
        app.opener().reveal_item_in_dir(&path).map_err(err)?;
        return Ok("revealed");
    }
    // Opened files from the internet get Windows' usual warnings (Office protected view...)
    let info_hash = engine.handle(id).map_err(err)?.info_hash().as_string();
    if !state.library.is_shared(&info_hash) {
        posters::mark_of_the_web(&path);
    }
    app.opener()
        .open_path(path.to_string_lossy(), None::<&str>)
        .map_err(err)?;
    Ok("opened")
}

#[tauri::command]
async fn show_in_folder(app: AppHandle, state: State<'_, AppState>, id: usize) -> CmdResult<()> {
    let engine = state.engine().await?;
    let handle = engine.handle(id).map_err(err)?;
    // Single-file torrents: select the file; others: open the torrent's folder
    let single = engine
        .files(id)
        .map(|f| f.iter().filter(|f| !f.padding).count() == 1)
        .unwrap_or(false);
    let target = match single.then(|| engine.file_path(id, 0)) {
        Some(Ok(path)) if path.exists() => path,
        _ => handle.output_folder().to_path_buf(),
    };
    app.opener().reveal_item_in_dir(target).map_err(err)
}

#[tauri::command]
fn open_downloads(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    let dir = state.prefs().download_dir;
    std::fs::create_dir_all(&dir).ok();
    app.opener().open_path(dir, None::<&str>).map_err(err)
}

#[tauri::command]
async fn magnet_link(state: State<'_, AppState>, id: usize) -> CmdResult<String> {
    let engine = state.engine().await?;
    let handle = engine.handle(id).map_err(err)?;
    let mut link = format!("magnet:?xt=urn:btih:{}", handle.info_hash().as_string());
    if let Some(name) = handle.name() {
        link.push_str("&dn=");
        link.push_str(&url_component(&name));
    }
    Ok(link)
}

#[tauri::command]
async fn save_torrent_file(
    app: AppHandle,
    state: State<'_, AppState>,
    id: usize,
) -> CmdResult<bool> {
    let engine = state.engine().await?;
    let bytes = engine.torrent_bytes(id).map_err(err)?;
    let name = engine
        .handle(id)
        .ok()
        .and_then(|h| h.name())
        .unwrap_or_else(|| "torrent".into());
    let picked = app
        .dialog()
        .file()
        .set_title(i18n::t(state.lang(), "dialog.saveTorrent"))
        .set_file_name(format!("{}.torrent", sanitize_file_name(&name)))
        .add_filter(i18n::t(state.lang(), "filter.torrent"), &["torrent"])
        .blocking_save_file();
    let Some(path) = picked.and_then(|p| p.into_path().ok()) else {
        return Ok(false);
    };
    std::fs::write(path, bytes).map_err(err)?;
    Ok(true)
}

#[tauri::command]
async fn set_prefs(app: AppHandle, state: State<'_, AppState>, prefs: Prefs) -> CmdResult<Prefs> {
    let old = state.prefs();
    prefs.save(&state.data_dir).map_err(err)?;
    *state.prefs.lock().unwrap() = prefs.clone();
    if old.download_limit != prefs.download_limit || old.upload_limit != prefs.upload_limit {
        if let Ok(engine) = state.engine().await {
            engine.set_limits(&prefs);
        }
    }
    if old.language != prefs.language {
        if let (Some(tray), Ok(menu)) = (app.tray_by_id("main"), tray_menu(&app)) {
            tray.set_menu(Some(menu)).ok();
        }
    }
    Ok(prefs)
}

#[tauri::command]
fn choose_download_dir(app: AppHandle, state: State<'_, AppState>) -> CmdResult<Option<Prefs>> {
    let current = state.prefs().download_dir;
    let picked = app
        .dialog()
        .file()
        .set_title(i18n::t(state.lang(), "dialog.downloadDir"))
        .set_directory(&current)
        .blocking_pick_folder();
    let Some(dir) = picked.and_then(|p| p.into_path().ok()) else {
        return Ok(None);
    };
    let mut prefs = state.prefs();
    prefs.download_dir = dir.to_string_lossy().into_owned();
    prefs.save(&state.data_dir).map_err(err)?;
    *state.prefs.lock().unwrap() = prefs.clone();
    Ok(Some(prefs))
}

#[tauri::command]
fn set_start_on_login(app: AppHandle, enabled: bool) -> CmdResult<bool> {
    let launcher = app.autolaunch();
    if enabled {
        launcher.enable().map_err(err)?;
    } else {
        launcher.disable().map_err(err)?;
    }
    launcher.is_enabled().map_err(err)
}

#[tauri::command]
fn make_default_app(app: AppHandle) -> CmdResult<bool> {
    register_handlers(&app).map_err(err)?;
    Ok(is_default_app())
}

#[tauri::command]
fn quit(app: AppHandle) {
    quit_app(&app);
}

// ---------------------------------------------------------------------------
// App setup

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            show_main_window(app);
            open_args(app, argv.into_iter().skip(1).collect());
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--hidden"]),
        ))
        .setup(|app| {
            let data_dir = data_dir(app.handle());
            std::fs::create_dir_all(&data_dir).ok();
            let mut prefs = Prefs::load(&data_dir);

            // Offer magnet links and .torrent files to this app the first time it runs
            if !prefs.handlers_set_up {
                if register_handlers(app.handle()).is_ok() {
                    prefs.handlers_set_up = true;
                }
                prefs.save(&data_dir).ok();
            }

            let mpv = player::mpv_path(app.path().resource_dir().ok());
            app.manage(AppState {
                library: library::Library::load(&data_dir),
                keys: keys::KeyStore::new(&data_dir),
                data_dir,
                mpv,
                prefs: Mutex::new(prefs),
                engine: tokio::sync::OnceCell::new(),
                translate_cancel: AtomicBool::new(false),
            });

            build_tray(app.handle())?;

            // Start the engine right away, then open what we were launched with
            let args: Vec<String> = std::env::args().skip(1).collect();
            let hidden = args.iter().any(|a| a == "--hidden");
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let state = handle.state::<AppState>();
                if let Err(e) = state.engine().await {
                    handle.emit("engine-error", e).ok();
                    return;
                }
                open_args(&handle, args);
                tauri::async_runtime::spawn(media_cmds::poster_worker(handle.clone()));
                watch_torrents(handle.clone()).await;
            });

            if !hidden {
                show_main_window(app.handle());
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // In the tray, the window really closes: that ends its WebView2 processes,
            // about 150 MB, and the torrent engine keeps running. The tray reopens it.
            if let WindowEvent::CloseRequested { api, .. } = event {
                let state = window.state::<AppState>();
                if !state.prefs().minimize_to_tray {
                    api.prevent_close();
                    quit_app(window.app_handle());
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            app_info,
            snapshot,
            files,
            add,
            add_paths,
            add_bytes,
            choose_torrent_file,
            create_torrent,
            pause,
            resume,
            pause_all,
            resume_all,
            remove,
            set_files,
            play,
            open_file,
            show_in_folder,
            open_downloads,
            magnet_link,
            save_torrent_file,
            set_prefs,
            choose_download_dir,
            set_start_on_login,
            make_default_app,
            quit,
            media_cmds::library_items,
            media_cmds::set_watched,
            media_cmds::poster,
            media_cmds::pick_subtitle,
            media_cmds::open_subtitle,
            media_cmds::translate_subtitle,
            media_cmds::cancel_translation,
            media_cmds::ollama_models,
            media_cmds::torrent_subtitles,
            media_cmds::reveal_path,
            media_cmds::set_api_key,
            media_cmds::saved_api_keys,
            media_cmds::open_help,
        ])
        .build(tauri::generate_context!())
        .expect("error while building the app")
        .run(|_app, event| {
            if let RunEvent::ExitRequested { api, code, .. } = event {
                // Closing the last window shouldn't quit while we live in the tray
                if code.is_none() {
                    api.prevent_exit();
                }
            }
        });
}

/// Settings, session and the window's WebView2 data go next to the executable in
/// the portable version (a "Portable Settings" folder beside it), else in the
/// user's local app data folder. FLOWBIT_DATA_DIR overrides both, for tests.
fn data_dir(app: &AppHandle) -> PathBuf {
    if let Some(dir) = std::env::var_os("FLOWBIT_DATA_DIR") {
        return PathBuf::from(dir);
    }
    if let Some(portable) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|d| d.join("Portable Settings")))
        .filter(|d| d.is_dir())
    {
        return portable;
    }
    app.path()
        .app_local_data_dir()
        .unwrap_or_else(|_| PathBuf::from("Flowbit"))
}

/// Shows the window, creating it if it was closed to the tray
fn show_main_window(app: &AppHandle) {
    let window = match app.get_webview_window("main") {
        Some(window) => window,
        None => {
            let Some(config) = app.config().app.windows.iter().find(|w| w.label == "main") else {
                return;
            };
            let webview_data = app.state::<AppState>().data_dir.join("EBWebView");
            match tauri::WebviewWindowBuilder::from_config(app, config)
                .map(|b| {
                    b.data_directory(webview_data)
                        // The window only ever shows the app itself: no links out,
                        // whatever a torrent name or file might contain
                        .on_navigation(|url| {
                            matches!(url.scheme(), "tauri")
                                || url.host_str() == Some("tauri.localhost")
                        })
                })
                .and_then(|b| b.build())
            {
                Ok(window) => {
                    #[cfg(windows)]
                    drop::install(&window);
                    window
                }
                Err(e) => {
                    eprintln!("couldn't open the window: {e}");
                    return;
                }
            }
        }
    };
    window.unminimize().ok();
    window.show().ok();
    window.set_focus().ok();
}

/// Adds magnet links, URLs and .torrent files passed on the command line
fn open_args(app: &AppHandle, args: Vec<String>) {
    let sources: Vec<String> = args
        .into_iter()
        .filter(|a| !a.starts_with("--"))
        .filter(|a| engine::is_url_like(a) || a.to_lowercase().ends_with(".torrent"))
        .collect();
    if sources.is_empty() {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let Ok(engine) = state.engine().await else {
            return;
        };
        for source in sources {
            match engine.add(&source, None).await {
                Ok(id) => app.emit("torrent-added", id).ok(),
                Err(e) => app.emit("error", format!("{e:#}")).ok(),
            };
        }
    });
}

/// Tells downloads finishing now from finished torrents being reloaded at startup.
/// While a restored torrent is being checked it reports "not finished", so it only
/// counts once it has been seen missing data after the check.
#[derive(Default)]
struct FinishNotices {
    restored: HashSet<String>,
    incomplete: HashSet<String>,
}

impl FinishNotices {
    fn observe(&mut self, info_hash: &str, state: &str, finished: bool) {
        if !finished && matches!(state, "live" | "paused") {
            self.incomplete.insert(info_hash.to_owned());
        }
    }

    fn downloaded_now(&self, info_hash: &str) -> bool {
        !self.restored.contains(info_hash) || self.incomplete.contains(info_hash)
    }
}

/// Notifies when downloads finish, pauses them if they shouldn't seed, and keeps
/// the tray tooltip's speeds current
async fn watch_torrents(app: AppHandle) {
    let mut finished: HashSet<String> = HashSet::new();
    let mut notices = FinishNotices::default();
    let mut first = true;
    let mut last_count = 0;
    loop {
        let state = app.state::<AppState>();
        let Ok(engine) = state.engine().await else {
            return;
        };
        let torrents = engine.list();
        let prefs = state.prefs();
        if first {
            notices.restored = torrents.iter().map(|t| t.info_hash.clone()).collect();
        }
        for t in &torrents {
            notices.observe(&t.info_hash, t.state, t.finished);
            if t.state == "live" && stops_when_done(&state, &prefs, t) {
                engine.pause(t.id).await.ok();
            }
            if t.finished && t.has_metadata && finished.insert(t.info_hash.clone()) {
                // Downloaded files get the Mark of the Web, like browser downloads
                if !state.library.is_shared(&t.info_hash) {
                    let paths: Vec<PathBuf> = engine
                        .files(t.id)
                        .map(|files| {
                            files
                                .iter()
                                .filter(|f| !f.padding)
                                .filter_map(|f| engine.file_path(t.id, f.index).ok())
                                .collect()
                        })
                        .unwrap_or_default();
                    tauri::async_runtime::spawn_blocking(move || {
                        paths.iter().for_each(|p| posters::mark_of_the_web(p))
                    });
                }
                if !notices.downloaded_now(&t.info_hash) {
                    continue;
                }
                let name = t.name.clone().unwrap_or_else(|| "Torrent".into());
                if prefs.notifications {
                    app.notification()
                        .builder()
                        .title(i18n::t(state.lang(), "notify.done"))
                        .body(&name)
                        .show()
                        .ok();
                }
                app.emit("torrent-done", name).ok();
            }
        }
        finished.retain(|hash| torrents.iter().any(|t| &t.info_hash == hash));
        if first || torrents.len() != last_count {
            state
                .library
                .retain(&torrents.iter().map(|t| t.info_hash.clone()).collect());
        }
        last_count = torrents.len();
        first = false;

        if let Some(tray) = app.tray_by_id("main") {
            let down: u64 = torrents.iter().map(|t| t.down_speed).sum();
            let up: u64 = torrents.iter().map(|t| t.up_speed).sum();
            tray.set_tooltip(Some(format!(
                "Flowbit\n↓ {}/s  ↑ {}/s",
                format_bytes(down),
                format_bytes(up)
            )))
            .ok();
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let menu = tray_menu(app)?;
    TrayIconBuilder::with_id("main")
        .icon(app.default_window_icon().cloned().expect("app icon"))
        .tooltip("Flowbit")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        })
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => show_main_window(app),
            "pause-all" | "resume-all" => {
                let pause = event.id().as_ref() == "pause-all";
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    let state = app.state::<AppState>();
                    if let Ok(engine) = state.engine().await {
                        if pause {
                            for t in engine.list() {
                                engine.pause(t.id).await.ok();
                            }
                        } else {
                            resume_unfinished(&state, &engine).await;
                        }
                    }
                });
            }
            "quit" => quit_app(app),
            _ => {}
        })
        .build(app)?;
    Ok(())
}

fn tray_menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let lang = app.state::<AppState>().lang();
    Menu::with_items(
        app,
        &[
            &MenuItem::with_id(app, "show", i18n::t(lang, "tray.show"), true, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(
                app,
                "pause-all",
                i18n::t(lang, "tray.pause"),
                true,
                None::<&str>,
            )?,
            &MenuItem::with_id(
                app,
                "resume-all",
                i18n::t(lang, "tray.resume"),
                true,
                None::<&str>,
            )?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "quit", i18n::t(lang, "tray.quit"), true, None::<&str>)?,
        ],
    )
}

/// Saves the session and quits, waiting at most a few seconds
fn quit_app(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        if let Some(Ok(engine)) = state.engine.get() {
            let stop = engine.session().stop();
            tokio::time::timeout(Duration::from_secs(4), stop)
                .await
                .ok();
        }
        app.exit(0);
    });
}

fn register_handlers(app: &AppHandle) -> anyhow::Result<()> {
    #[cfg(any(windows, target_os = "linux"))]
    {
        let exe = std::env::current_exe()?;
        let icon = app
            .path()
            .resource_dir()
            .ok()
            .map(|d| d.join("resources").join("torrent-file.ico"))
            .filter(|p| p.exists());
        handlers::register(&exe, icon.as_deref())?;
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    let _ = app;
    Ok(())
}

fn is_default_app() -> bool {
    #[cfg(any(windows, target_os = "linux"))]
    {
        std::env::current_exe()
            .map(|exe| handlers::is_registered(&exe))
            .unwrap_or(false)
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    false
}

#[cfg(windows)]
fn disk_space(path: &Path) -> Option<(u64, u64)> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    extern "system" {
        fn GetDiskFreeSpaceExW(
            dir: *const u16,
            free_to_caller: *mut u64,
            total: *mut u64,
            total_free: *mut u64,
        ) -> i32;
    }
    // The folder might not exist yet: ask about the closest existing parent
    let existing = path.ancestors().find(|p| p.exists())?;
    let wide: Vec<u16> = existing.as_os_str().encode_wide().chain(Some(0)).collect();
    let (mut free, mut total, mut total_free) = (0u64, 0u64, 0u64);
    let ok = unsafe { GetDiskFreeSpaceExW(wide.as_ptr(), &mut free, &mut total, &mut total_free) };
    (ok != 0).then_some((free, total))
}

// statvfs field types differ between platforms (u32 or u64)
#[cfg(unix)]
#[allow(clippy::unnecessary_cast)]
fn disk_space(path: &Path) -> Option<(u64, u64)> {
    use std::os::unix::ffi::OsStrExt;
    let existing = path.ancestors().find(|p| p.exists())?;
    let c_path = std::ffi::CString::new(existing.as_os_str().as_bytes()).ok()?;
    let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(c_path.as_ptr(), &mut st) } != 0 {
        return None;
    }
    let block = st.f_frsize as u64;
    Some((st.f_bavail as u64 * block, st.f_blocks as u64 * block))
}

#[cfg(not(any(windows, unix)))]
fn disk_space(_path: &Path) -> Option<(u64, u64)> {
    None
}

fn format_bytes(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = n as f64;
    let mut unit = 0;
    while value >= 1000.0 && unit < UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{n} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

fn url_component(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

fn sanitize_file_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if "<>:\"/\\|?*".contains(c) || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn restored() -> FinishNotices {
        FinishNotices {
            restored: HashSet::from(["a".to_owned()]),
            ..Default::default()
        }
    }

    #[test]
    fn a_finished_torrent_checked_at_startup_is_not_announced() {
        let mut notices = restored();
        notices.observe("a", "initializing", false);
        notices.observe("a", "paused", true);
        assert!(!notices.downloaded_now("a"));
    }

    #[test]
    fn a_restored_download_that_completes_is_announced() {
        let mut notices = restored();
        notices.observe("a", "initializing", false);
        notices.observe("a", "live", false);
        notices.observe("a", "live", true);
        assert!(notices.downloaded_now("a"));
    }

    #[test]
    fn a_restored_torrent_that_failed_is_not_counted_as_incomplete() {
        let mut notices = restored();
        notices.observe("a", "error", false);
        notices.observe("a", "paused", true);
        assert!(!notices.downloaded_now("a"));
    }

    #[test]
    fn a_torrent_added_while_running_is_announced() {
        let mut notices = restored();
        notices.observe("b", "live", true);
        assert!(notices.downloaded_now("b"));
    }
}
