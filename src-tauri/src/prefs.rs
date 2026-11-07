use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// User preferences, saved as JSON in the app's data folder.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default, rename_all = "camelCase")]
pub struct Prefs {
    pub download_dir: String,
    /// KB/s, 0 means unlimited
    pub download_limit: u32,
    pub upload_limit: u32,
    /// "system", "light" or "dark"
    pub theme: String,
    pub minimize_to_tray: bool,
    /// Pause torrents when they finish downloading, instead of seeding them
    pub stop_seeding_when_done: bool,
    pub sounds: bool,
    pub notifications: bool,
    pub sort_by_name: bool,
    /// Preferred subtitle languages for mpv, comma separated
    pub subtitle_languages: String,
    /// Preferred audio languages for mpv, comma separated (empty: the file's default)
    pub audio_languages: String,
    /// 0 picks a free port at random
    pub listen_port: u16,
    pub trackers: Vec<String>,
    /// Whether we already offered to become the default torrent app
    pub handlers_set_up: bool,
    /// "system", "en" or "ar"
    pub language: String,
    /// Subtitle translator: "google", "microsoft" or "ollama"
    pub sub_engine: String,
    pub sub_target: String,
    pub sub_model: String,
    /// Address of a LibreTranslate server (usually one running on this computer)
    pub sub_libre_url: String,
    /// AI service for the "ai" engine: gemini, groq, openrouter, openai, ollama, custom
    pub sub_ai_provider: String,
    /// Model name; empty uses the service's default
    pub sub_ai_model: String,
    /// Chat completions address for the "custom" AI service
    pub sub_ai_url: String,
    /// Arabic style for AI translations: msa, gulf, egyptian, levantine
    pub sub_arabic_style: String,
    /// Names and terms the AI keeps as written
    pub sub_glossary: String,
    pub sub_rtl_fix: bool,
    pub sub_bilingual: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            download_dir: default_download_dir().to_string_lossy().into_owned(),
            download_limit: 0,
            upload_limit: 0,
            theme: "system".into(),
            minimize_to_tray: true,
            stop_seeding_when_done: true,
            sounds: true,
            notifications: true,
            sort_by_name: true,
            subtitle_languages: "ar,ara,en,eng".into(),
            audio_languages: String::new(),
            listen_port: 0,
            trackers: Vec::new(),
            handlers_set_up: false,
            language: "system".into(),
            sub_engine: "google".into(),
            sub_target: "ar".into(),
            sub_model: String::new(),
            sub_libre_url: "http://127.0.0.1:5000".into(),
            sub_ai_provider: "gemini".into(),
            sub_ai_model: String::new(),
            sub_ai_url: String::new(),
            sub_arabic_style: "msa".into(),
            sub_glossary: String::new(),
            sub_rtl_fix: true,
            sub_bilingual: false,
        }
    }
}

impl Prefs {
    pub fn load(data_dir: &Path) -> Self {
        let mut prefs: Self = std::fs::read(path(data_dir))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        // 1.1 had Ollama as its own engine; it's one of the AI services now
        if prefs.sub_engine == "ollama" {
            prefs.sub_engine = "ai".into();
            prefs.sub_ai_provider = "ollama".into();
            if prefs.sub_ai_model.is_empty() {
                prefs.sub_ai_model = prefs.sub_model.clone();
            }
        }
        prefs
    }

    pub fn save(&self, data_dir: &Path) -> anyhow::Result<()> {
        std::fs::create_dir_all(data_dir)?;
        // Write to a temporary file first, so a crash can't leave half a file
        let tmp = data_dir.join("prefs.json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(tmp, path(data_dir))?;
        Ok(())
    }
}

fn path(data_dir: &Path) -> PathBuf {
    data_dir.join("prefs.json")
}

fn default_download_dir() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .map(|home| PathBuf::from(home).join("Downloads"))
        .unwrap_or_else(|| PathBuf::from("Downloads"))
}
