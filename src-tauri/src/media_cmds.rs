//! Commands for the watch library, posters and the subtitle translator

use std::{
    path::{Path, PathBuf},
    sync::atomic::Ordering,
};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;

use crate::{
    engine::FileView, err, i18n, library, player, posters, subtitles, translate, AppState,
    CmdResult,
};

// ---------------------------------------------------------------------------
// Watch library

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryItem {
    torrent_id: usize,
    info_hash: String,
    torrent_name: Option<String>,
    file_index: usize,
    /// Path inside the torrent, like "Season 1/Show - 05.mkv"
    name: String,
    length: u64,
    downloaded: u64,
    is_video: bool,
    torrent_state: &'static str,
    #[serde(flatten)]
    watch: library::WatchState,
    /// Subtitle files next to it in the torrent
    subtitles: Vec<usize>,
}

/// Every video and audio file in every torrent, with what you've watched
#[tauri::command]
pub async fn library_items(state: State<'_, AppState>) -> CmdResult<Vec<LibraryItem>> {
    let engine = state.engine().await?;
    let watched = state.library.all();
    let mut items = Vec::new();
    for t in engine.list() {
        let Ok(files) = engine.files(t.id) else {
            continue;
        };
        let subtitle_files: Vec<&FileView> = files
            .iter()
            .filter(|f| !f.padding && subtitles::Format::from_path(Path::new(&f.name)).is_some())
            .collect();
        for f in files
            .iter()
            .filter(|f| !f.padding && player::is_media(&f.name))
        {
            let stem = Path::new(&f.name)
                .file_stem()
                .map(|s| s.to_string_lossy().to_lowercase());
            let subs = subtitle_files
                .iter()
                .filter(|s| {
                    let s_stem = Path::new(&s.name)
                        .file_stem()
                        .map(|x| x.to_string_lossy().to_lowercase());
                    match (&stem, s_stem) {
                        (Some(v), Some(s)) => s.starts_with(v.as_str()),
                        _ => false,
                    }
                })
                .map(|s| s.index)
                .collect();
            items.push(LibraryItem {
                torrent_id: t.id,
                info_hash: t.info_hash.clone(),
                torrent_name: t.name.clone(),
                file_index: f.index,
                name: f.name.clone(),
                length: f.length,
                downloaded: f.progress,
                is_video: player::is_video(&f.name),
                torrent_state: t.state,
                watch: watched
                    .get(&library::key(&t.info_hash, f.index))
                    .cloned()
                    .unwrap_or_default(),
                subtitles: subs,
            });
        }
    }
    Ok(items)
}

#[tauri::command]
pub fn set_watched(state: State<'_, AppState>, info_hash: String, file: usize, watched: bool) {
    state.library.update(&library::key(&info_hash, file), |s| {
        s.watched = watched;
        if !watched {
            s.position = 0.0;
        }
    });
}

#[tauri::command]
pub fn poster(state: State<'_, AppState>, info_hash: String) -> Option<String> {
    if !info_hash.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    posters::data_url(&posters::path(&state.data_dir, &info_hash))
}

/// Makes posters for torrents that have a video (or audio with cover art), one at a
/// time, once enough of the file is there. Runs in the background for the app's life.
pub async fn poster_worker(app: AppHandle) {
    let mut tried = std::collections::HashSet::new();
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(8)).await;
        let state = app.state::<AppState>();
        let Ok(engine) = state.engine().await else {
            return;
        };
        for t in engine.list() {
            if !t.has_metadata || tried.contains(&t.info_hash) {
                continue;
            }
            let out = posters::path(&state.data_dir, &t.info_hash);
            if out.exists() {
                tried.insert(t.info_hash.clone());
                continue;
            }
            let Ok(files) = engine.files(t.id) else {
                continue;
            };
            // The video that's furthest along (then the largest); else audio, for cover art
            let share = |f: &&FileView| (f.progress * 1000).checked_div(f.length).unwrap_or(0);
            let pick = |video: bool| {
                files
                    .iter()
                    .filter(|f| {
                        !f.padding
                            && f.included
                            && if video {
                                player::is_video(&f.name)
                            } else {
                                player::is_media(&f.name)
                            }
                    })
                    .max_by_key(|f| (share(f), f.length))
            };
            let Some(main) = pick(true).or_else(|| pick(false)) else {
                tried.insert(t.info_hash.clone());
                continue;
            };
            // Wait until some of it is there, unless the torrent is complete
            let enough = t.finished || share(&main) >= 50;
            if !enough || t.state != "live" && !t.finished {
                continue;
            }
            tried.insert(t.info_hash.clone());
            let url = engine.stream_url(t.id, main.index, &main.name);
            if posters::generate(&state.mpv, &url, &out).await.is_ok() {
                app.emit("poster", t.info_hash.clone()).ok();
            }
            break; // one per round, so playback keeps the bandwidth
        }
    }
}

// ---------------------------------------------------------------------------
// Subtitle translator

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubtitleInfo {
    path: String,
    name: String,
    format: &'static str,
    cues: usize,
    preview: Vec<String>,
}

fn subtitle_info(path: &Path) -> anyhow::Result<SubtitleInfo> {
    let doc = subtitles::read(path)?;
    Ok(SubtitleInfo {
        path: path.to_string_lossy().into_owned(),
        name: path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        format: doc.format.name(),
        cues: doc.cues.len(),
        preview: doc.preview(6),
    })
}

#[tauri::command]
pub fn pick_subtitle(
    app: AppHandle,
    state: State<'_, AppState>,
) -> CmdResult<Option<SubtitleInfo>> {
    let lang = i18n::language(&state.prefs().language);
    let picked = app
        .dialog()
        .file()
        .set_title(i18n::t(lang, "dialog.openSubtitle"))
        .add_filter(
            i18n::t(lang, "filter.subtitles"),
            &["srt", "ass", "ssa", "vtt"],
        )
        .blocking_pick_file();
    let Some(path) = picked.and_then(|p| p.into_path().ok()) else {
        return Ok(None);
    };
    subtitle_info(&path).map(Some).map_err(err)
}

/// A subtitle file dropped on the window, or from a torrent
#[tauri::command]
pub async fn open_subtitle(
    state: State<'_, AppState>,
    path: Option<String>,
    torrent: Option<(usize, usize)>,
) -> CmdResult<SubtitleInfo> {
    let path = match (path, torrent) {
        (Some(p), _) => PathBuf::from(p),
        (None, Some((id, file))) => {
            let engine = state.engine().await?;
            engine.file_path(id, file).map_err(err)?
        }
        _ => return Err("no subtitle file".into()),
    };
    subtitle_info(&path).map_err(err)
}

/// The engine, language and options come from the preferences (the window saves
/// them first); keys from the encrypted key store
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslateRequest {
    path: String,
    /// Ask where to save instead of saving next to the original
    save_as: bool,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Progress {
    done: usize,
    total: usize,
}

/// Translates a subtitle file and saves it with the same timings and formatting.
/// Sends "subtitle-progress" events; returns the saved file's path.
#[tauri::command]
pub async fn translate_subtitle(
    app: AppHandle,
    state: State<'_, AppState>,
    request: TranslateRequest,
) -> CmdResult<Option<String>> {
    let source = PathBuf::from(&request.path);
    let doc = subtitles::read(&source).map_err(err)?;
    if doc.cues.is_empty() {
        return Err("no subtitles in this file".into());
    }
    if doc.cues.len() > 20_000 {
        return Err("too many subtitles in this file".into());
    }

    let p = state.prefs();
    let key_service = match p.sub_engine.as_str() {
        "deepl" => "deepl",
        "ai" => p.sub_ai_provider.as_str(),
        _ => "",
    };
    let engine = translate::Engine::from_choice(translate::EngineChoice {
        engine: &p.sub_engine,
        ai_provider: &p.sub_ai_provider,
        ai_model: &p.sub_ai_model,
        ai_url: &p.sub_ai_url,
        libre_url: &p.sub_libre_url,
        key: state.keys.get(key_service),
    })
    .map_err(err)?;
    let style = translate::Style {
        arabic_dialect: p.sub_arabic_style.clone(),
        glossary: p.sub_glossary.clone(),
    };
    let mut translator = translate::Translator::new(engine, &p.sub_target, style).map_err(err)?;
    let target = p.sub_target.clone();

    // Every unit of every cue, in order; empty ones skip the translator
    let units: Vec<String> = doc
        .cues
        .iter()
        .flat_map(|c| c.units.iter().cloned())
        .collect();
    let mut translated: Vec<String> = vec![String::new(); units.len()];
    let todo: Vec<usize> = (0..units.len())
        .filter(|&i| !units[i].trim().is_empty())
        .collect();
    let texts: Vec<String> = todo.iter().map(|&i| units[i].clone()).collect();

    state.translate_cancel.store(false, Ordering::SeqCst);
    let total = texts.len();
    app.emit("subtitle-progress", Progress { done: 0, total })
        .ok();
    for range in translator.batches(&texts) {
        if state.translate_cancel.load(Ordering::SeqCst) {
            return Err("cancelled".into());
        }
        let out = translator
            .translate(&texts[range.clone()])
            .await
            .map_err(|e| format!("{e:#}"))?;
        for (k, text) in range.clone().zip(out) {
            // Translations are plain text: braces and angle brackets would read as
            // ASS override tags or HTML formatting in the output file
            translated[todo[k]] = text
                .chars()
                .map(|c| match c {
                    '{' | '<' => '(',
                    '}' | '>' => ')',
                    '\r' | '\n' => ' ',
                    c => c,
                })
                .collect();
        }
        app.emit(
            "subtitle-progress",
            Progress {
                done: range.end,
                total,
            },
        )
        .ok();
    }

    // Back into cues
    let mut per_cue = Vec::with_capacity(doc.cues.len());
    let mut it = translated.into_iter();
    for cue in &doc.cues {
        per_cue.push(it.by_ref().take(cue.units.len()).collect::<Vec<_>>());
    }
    let rtl = p.sub_rtl_fix && translate::is_rtl(&target);
    let output = doc.render(&per_cue, rtl, p.sub_bilingual);

    let out_path = if request.save_as {
        let lang = i18n::language(&p.language);
        let suggested = subtitles::output_path(&source, &target);
        let picked = app
            .dialog()
            .file()
            .set_title(i18n::t(lang, "dialog.saveSubtitle"))
            .set_file_name(
                suggested
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default(),
            )
            .add_filter(
                i18n::t(lang, "filter.subtitles"),
                &["srt", "ass", "ssa", "vtt"],
            )
            .blocking_save_file();
        match picked.and_then(|p| p.into_path().ok()) {
            Some(p) => p,
            None => return Ok(None),
        }
    } else {
        subtitles::output_path(&source, &target)
    };
    std::fs::write(&out_path, output.as_bytes()).map_err(err)?;
    Ok(Some(out_path.to_string_lossy().into_owned()))
}

/// Saves an API key (encrypted), or removes it when empty. Returns the services with keys.
#[tauri::command]
pub fn set_api_key(
    state: State<'_, AppState>,
    service: String,
    key: String,
) -> CmdResult<Vec<String>> {
    state.keys.set(&service, &key).map_err(err)?;
    Ok(state.keys.saved())
}

#[tauri::command]
pub fn saved_api_keys(state: State<'_, AppState>) -> Vec<String> {
    state.keys.saved()
}

/// Opens one of the setup pages in the browser. Only these: the window never opens
/// an address it was given.
#[tauri::command]
pub fn open_help(app: AppHandle, page: String) -> CmdResult<()> {
    use tauri_plugin_opener::OpenerExt;
    let url = match page.as_str() {
        "gemini-key" => "https://aistudio.google.com/apikey",
        "groq-key" => "https://console.groq.com/keys",
        "openrouter-key" => "https://openrouter.ai/keys",
        "openrouter-models" => "https://openrouter.ai/models?q=free",
        "openai-key" => "https://platform.openai.com/api-keys",
        "deepl-signup" => "https://www.deepl.com/pro-api",
        "deepl-keys" => "https://www.deepl.com/your-account/keys",
        "ollama" => "https://ollama.com/download",
        "ollama-models" => "https://ollama.com/library",
        "libretranslate" => "https://github.com/LibreTranslate/LibreTranslate#install-and-run",
        "python" => "https://www.python.org/downloads/",
        "docker" => "https://www.docker.com/products/docker-desktop/",
        _ => return Err("unknown page".into()),
    };
    app.opener().open_url(url, None::<&str>).map_err(err)
}

/// Shows a translated subtitle file in its folder (only subtitle files)
#[tauri::command]
pub fn reveal_path(app: AppHandle, path: String) -> CmdResult<()> {
    use tauri_plugin_opener::OpenerExt;
    let path = PathBuf::from(path);
    if !path.is_file() || subtitles::Format::from_path(&path).is_none() {
        return Err("not a subtitle file".into());
    }
    app.opener().reveal_item_in_dir(path).map_err(err)
}

#[tauri::command]
pub fn cancel_translation(state: State<'_, AppState>) {
    state.translate_cancel.store(true, Ordering::SeqCst);
}

#[tauri::command]
pub async fn ollama_models() -> CmdResult<Vec<String>> {
    translate::ollama_models()
        .await
        .map_err(|e| format!("{e:#}"))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TorrentSubtitle {
    torrent_id: usize,
    file_index: usize,
    name: String,
    complete: bool,
}

/// Subtitle files inside torrents, for translating them without looking for them
#[tauri::command]
pub async fn torrent_subtitles(state: State<'_, AppState>) -> CmdResult<Vec<TorrentSubtitle>> {
    let engine = state.engine().await?;
    let mut out = Vec::new();
    for t in engine.list() {
        let Ok(files) = engine.files(t.id) else {
            continue;
        };
        for f in files {
            if !f.padding && subtitles::Format::from_path(Path::new(&f.name)).is_some() {
                out.push(TorrentSubtitle {
                    torrent_id: t.id,
                    file_index: f.index,
                    complete: f.length > 0 && f.progress >= f.length,
                    name: f.name,
                });
            }
        }
    }
    Ok(out)
}
