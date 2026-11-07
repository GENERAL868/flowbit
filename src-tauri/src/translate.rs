//! Subtitle translation engines. All free to use, none pretends to be a browser:
//! - Google Translate's free endpoint (translate.googleapis.com, client=gtx), no setup
//! - DeepL API Free, with your own free key (500,000 characters a month)
//! - AI models through the OpenAI-compatible chat API: Google Gemini (free tier),
//!   Groq (free), OpenRouter (free models), OpenAI, any compatible server, or Ollama
//!   on this computer. These read neighbouring lines, so translations fit the scene.
//! - LibreTranslate, the open-source translation server, run on your own computer

use std::time::Duration;

use anyhow::{bail, Context};
use serde_json::{json, Value};

/// AI services with an OpenAI-compatible chat endpoint
pub struct AiProvider {
    pub id: &'static str,
    pub url: &'static str,
    pub default_model: &'static str,
    pub needs_key: bool,
}

pub const AI_PROVIDERS: &[AiProvider] = &[
    AiProvider {
        id: "gemini",
        url: "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions",
        default_model: "gemini-2.5-flash",
        needs_key: true,
    },
    AiProvider {
        id: "groq",
        url: "https://api.groq.com/openai/v1/chat/completions",
        default_model: "llama-3.3-70b-versatile",
        needs_key: true,
    },
    AiProvider {
        id: "openrouter",
        url: "https://openrouter.ai/api/v1/chat/completions",
        default_model: "meta-llama/llama-3.3-70b-instruct:free",
        needs_key: true,
    },
    AiProvider {
        id: "openai",
        url: "https://api.openai.com/v1/chat/completions",
        default_model: "gpt-4o-mini",
        needs_key: true,
    },
    AiProvider {
        id: "ollama",
        url: "http://127.0.0.1:11434/v1/chat/completions",
        default_model: "",
        needs_key: false,
    },
    AiProvider { id: "custom", url: "", default_model: "", needs_key: false },
];

pub fn ai_provider(id: &str) -> Option<&'static AiProvider> {
    AI_PROVIDERS.iter().find(|p| p.id == id)
}

#[derive(Clone, Debug)]
pub enum Engine {
    Google,
    DeepL { key: String },
    Ai { url: String, model: String, key: Option<String>, label: &'static str },
    LibreTranslate { url: String },
}

/// What the user chose in the translator, plus their saved key for that service
pub struct EngineChoice<'a> {
    pub engine: &'a str,
    pub ai_provider: &'a str,
    pub ai_model: &'a str,
    pub ai_url: &'a str,
    pub libre_url: &'a str,
    pub key: Option<String>,
}

impl Engine {
    pub fn from_choice(c: EngineChoice) -> anyhow::Result<Self> {
        Ok(match c.engine {
            "google" => Self::Google,
            "deepl" => Self::DeepL { key: c.key.context("add your DeepL API Free key first")? },
            "ai" => {
                let p = ai_provider(c.ai_provider).context("choose an AI service")?;
                let url = if p.id == "custom" { c.ai_url.trim().to_owned() } else { p.url.to_owned() };
                check_url(&url)?;
                let model = if c.ai_model.trim().is_empty() { p.default_model } else { c.ai_model.trim() };
                anyhow::ensure!(!model.is_empty(), "choose a model first");
                if p.needs_key {
                    anyhow::ensure!(c.key.is_some(), "add your API key for this service first");
                }
                Self::Ai { url, model: model.to_owned(), key: c.key, label: p.id }
            }
            "libre" => {
                let url = c.libre_url.trim().trim_end_matches('/').to_owned();
                check_url(&url)?;
                Self::LibreTranslate { url }
            }
            _ => bail!("unknown translation engine"),
        })
    }

    /// How many texts and characters go in one request
    fn batch_limits(&self) -> (usize, usize) {
        match self {
            Self::Google => (60, 3500),
            Self::DeepL { .. } => (50, 20000),
            Self::Ai { .. } => (25, 3000),
            Self::LibreTranslate { .. } => (50, 5000),
        }
    }

    /// Servers on this computer may use plain http and take their time
    fn is_local(&self) -> bool {
        match self {
            Self::Ai { url, .. } | Self::LibreTranslate { url } => !url.starts_with("https://"),
            _ => false,
        }
    }
}

fn check_url(url: &str) -> anyhow::Result<()> {
    anyhow::ensure!(
        url.starts_with("https://") || url.starts_with("http://"),
        "the address should start with http:// or https://"
    );
    Ok(())
}

/// Target languages offered, Arabic first
pub const LANGUAGES: &[(&str, &str)] = &[
    ("ar", "Arabic"),
    ("en", "English"),
    ("fr", "French"),
    ("es", "Spanish"),
    ("de", "German"),
    ("tr", "Turkish"),
    ("fa", "Persian"),
    ("ur", "Urdu"),
    ("hi", "Hindi"),
    ("id", "Indonesian"),
    ("ms", "Malay"),
    ("pt", "Portuguese"),
    ("ru", "Russian"),
    ("it", "Italian"),
    ("ja", "Japanese"),
    ("ko", "Korean"),
    ("zh-CN", "Chinese (Simplified)"),
    ("bn", "Bengali"),
    ("nl", "Dutch"),
    ("pl", "Polish"),
    ("he", "Hebrew"),
];

pub fn is_rtl(lang: &str) -> bool {
    matches!(lang, "ar" | "fa" | "ur" | "he")
}

fn language_name(code: &str) -> &'static str {
    LANGUAGES.iter().find(|(c, _)| *c == code).map(|(_, n)| *n).unwrap_or("Arabic")
}

/// Options that make AI translations read better
#[derive(Default, Clone)]
pub struct Style {
    /// For Arabic: "msa", "gulf", "egyptian" or "levantine"
    pub arabic_dialect: String,
    /// Names and terms to keep, one per line or comma separated
    pub glossary: String,
}

pub struct Translator {
    engine: Engine,
    target: String,
    style: Style,
    client: reqwest::Client,
    /// Recent source and translated lines, sent to AI models as context
    context: Vec<(String, String)>,
}

impl Translator {
    pub fn new(engine: Engine, target: &str, style: Style) -> anyhow::Result<Self> {
        anyhow::ensure!(LANGUAGES.iter().any(|(c, _)| *c == target), "unsupported language");
        let local = engine.is_local();
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(if local { 300 } else { 90 }))
            .https_only(!local)
            .user_agent(concat!("Flowbit/",env!("CARGO_PKG_VERSION"), " (subtitle translator)"))
            .build()?;
        Ok(Self { engine, target: target.to_owned(), style, client, context: Vec::new() })
    }

    /// Splits texts into request-sized batches, keeping their order
    pub fn batches(&self, texts: &[String]) -> Vec<std::ops::Range<usize>> {
        let (max_items, max_chars) = self.engine.batch_limits();
        let mut out = Vec::new();
        let (mut start, mut chars) = (0, 0);
        for (i, t) in texts.iter().enumerate() {
            let len = t.chars().count();
            if i > start && (i - start >= max_items || chars + len > max_chars) {
                out.push(start..i);
                start = i;
                chars = 0;
            }
            chars += len;
        }
        if start < texts.len() {
            out.push(start..texts.len());
        }
        out
    }

    /// Translates one batch, returning exactly one translation per text
    pub async fn translate(&mut self, texts: &[String]) -> anyhow::Result<Vec<String>> {
        // Up to 4 tries; rate limits (HTTP 429) and hiccups wait longer each time
        let mut tries = 0u32;
        loop {
            let result = match self.engine.clone() {
                Engine::Google => self.google(texts).await,
                Engine::DeepL { key } => self.deepl(&key, texts).await,
                Engine::Ai { url, model, key, label } => self.ai(&url, &model, key.as_deref(), label, texts).await,
                Engine::LibreTranslate { url } => self.libre(&url, texts).await,
            };
            tries += 1;
            match result {
                Ok(out) => {
                    // Keep the last few lines as context for the next batch
                    self.context.extend(texts.iter().cloned().zip(out.iter().cloned()));
                    let excess = self.context.len().saturating_sub(6);
                    self.context.drain(..excess);
                    return Ok(out);
                }
                Err(e) if tries >= 4 => return Err(e),
                Err(_) => tokio::time::sleep(Duration::from_secs(2u64.pow(tries))).await,
            }
        }
    }

    async fn google(&self, texts: &[String]) -> anyhow::Result<Vec<String>> {
        // One request per batch, a line per text; if lines got merged, go one by one
        let joined = texts.join("\n");
        let out = self.google_one(&joined).await?;
        let lines: Vec<String> = out.split('\n').map(|s| s.trim().to_owned()).collect();
        if lines.len() == texts.len() {
            return Ok(lines);
        }
        let mut each = Vec::with_capacity(texts.len());
        for t in texts {
            each.push(self.google_one(t).await?.trim().to_owned());
        }
        Ok(each)
    }

    async fn google_one(&self, text: &str) -> anyhow::Result<String> {
        let url = format!(
            "https://translate.googleapis.com/translate_a/single?client=gtx&sl=auto&tl={}&dt=t",
            self.target
        );
        let resp = self
            .client
            .post(url)
            .header("Content-Type", "application/x-www-form-urlencoded;charset=utf-8")
            .body(format!("q={}", form_encode(text)))
            .send()
            .await?;
        anyhow::ensure!(resp.status().is_success(), "Google Translate: HTTP {}", resp.status());
        let v: Value = serde_json::from_slice(&resp.bytes().await?)?;
        let segments = v.get(0).and_then(|s| s.as_array()).context("Google Translate: unexpected reply")?;
        Ok(segments
            .iter()
            .filter_map(|s| s.get(0).and_then(|t| t.as_str()))
            .collect::<String>())
    }

    /// DeepL API: free keys end with ":fx" and use api-free.deepl.com
    async fn deepl(&self, key: &str, texts: &[String]) -> anyhow::Result<Vec<String>> {
        let host = if key.ends_with(":fx") { "api-free.deepl.com" } else { "api.deepl.com" };
        let target = match self.target.as_str() {
            "en" => "EN-US".to_owned(),
            "pt" => "PT-BR".to_owned(),
            "zh-CN" => "ZH-HANS".to_owned(),
            other => other.to_ascii_uppercase(),
        };
        let resp = self
            .client
            .post(format!("https://{host}/v2/translate"))
            .header("Authorization", format!("DeepL-Auth-Key {key}"))
            .json_body(&json!({ "text": texts, "target_lang": target }))?
            .send()
            .await?;
        match resp.status().as_u16() {
            200 => {}
            403 => bail!("DeepL: the key isn't valid"),
            456 => bail!("DeepL: this month's free characters are used up"),
            s => bail!("DeepL: HTTP {s}"),
        }
        let v: Value = serde_json::from_slice(&resp.bytes().await?)?;
        let out: Vec<String> = v["translations"]
            .as_array()
            .context("DeepL: unexpected reply")?
            .iter()
            .map(|t| t["text"].as_str().unwrap_or_default().to_owned())
            .collect();
        anyhow::ensure!(out.len() == texts.len(), "DeepL: missing translations");
        Ok(out)
    }

    fn ai_prompt(&self) -> String {
        let language = language_name(&self.target);
        let mut prompt = format!(
            "You are a professional subtitle translator. Translate the subtitles into {language}. \
             Each input line is NUMBER|TEXT. Reply with exactly one line per input line, as \
             NUMBER|TRANSLATION, same numbers, same order, and nothing else. Never merge, split, skip \
             or explain lines. Keep it natural, short and easy to read like real subtitles, keep the \
             tone (casual, rude, formal) of the original, and use the earlier lines to understand the scene."
        );
        if self.target == "ar" {
            prompt.push_str(match self.style.arabic_dialect.as_str() {
                "gulf" => " Write in natural Gulf Arabic (Khaleeji), as spoken in Saudi Arabia and the Gulf.",
                "egyptian" => " Write in natural Egyptian Arabic.",
                "levantine" => " Write in natural Levantine Arabic (Shami).",
                _ => " Write in clear Modern Standard Arabic, as used in professional subtitles.",
            });
        }
        let glossary: Vec<&str> = self
            .style
            .glossary
            .split([',', '\n', '،'])
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .take(100)
            .collect();
        if !glossary.is_empty() {
            prompt.push_str(&format!(
                " Keep these names and terms exactly as written: {}.",
                glossary.join(", ")
            ));
        }
        prompt
    }

    async fn ai(&self, url: &str, model: &str, key: Option<&str>, label: &str, texts: &[String]) -> anyhow::Result<Vec<String>> {
        let numbered: String = texts
            .iter()
            .enumerate()
            .map(|(i, t)| format!("{}|{}\n", i + 1, t))
            .collect();
        let mut messages = vec![json!({ "role": "system", "content": self.ai_prompt() })];
        if !self.context.is_empty() {
            let context: String = self
                .context
                .iter()
                .map(|(src, dst)| format!("{src} => {dst}\n"))
                .collect();
            messages.push(json!({
                "role": "user",
                "content": format!("Earlier lines and how they were translated (context only, don't repeat them):\n{context}")
            }));
        }
        messages.push(json!({ "role": "user", "content": numbered }));
        let mut req = self
            .client
            .post(url)
            .json_body(&json!({ "model": model, "temperature": 0.2, "messages": messages, "stream": false }))?;
        if let Some(key) = key {
            req = req.bearer_auth(key);
        }
        let name = ai_name(label);
        let resp = req.send().await.with_context(|| format!("{name}: couldn't connect"))?;
        match resp.status().as_u16() {
            200 => {}
            401 | 403 => bail!("{name}: the API key isn't valid"),
            404 => bail!("{name}: model \"{model}\" not found"),
            429 => bail!("{name}: too many requests or the free quota is used up, try again later"),
            s => bail!("{name}: HTTP {s}"),
        }
        let v: Value = serde_json::from_slice(&resp.bytes().await?)?;
        let content = v["choices"][0]["message"]["content"].as_str().unwrap_or_default();
        let mut out = vec![None; texts.len()];
        for line in content.lines() {
            let Some((n, text)) = line.trim().split_once('|') else { continue };
            if let Ok(n) = n.trim().trim_start_matches('#').parse::<usize>() {
                if (1..=texts.len()).contains(&n) && out[n - 1].is_none() {
                    out[n - 1] = Some(text.trim().to_owned());
                }
            }
        }
        // Lines the model skipped: translate them alone
        let mut result = Vec::with_capacity(texts.len());
        for (i, t) in out.into_iter().enumerate() {
            match t {
                Some(t) if !t.is_empty() => result.push(t),
                _ if texts.len() > 1 => {
                    result.push(Box::pin(self.ai(url, model, key, label, &texts[i..=i])).await?.remove(0))
                }
                _ => result.push(texts[i].clone()),
            }
        }
        Ok(result)
    }

    /// LibreTranslate takes a list and answers with a list, in the same order
    async fn libre(&self, url: &str, texts: &[String]) -> anyhow::Result<Vec<String>> {
        let target = match self.target.as_str() {
            "zh-CN" => "zh",
            other => other,
        };
        let resp = self
            .client
            .post(format!("{url}/translate"))
            .json_body(&json!({ "q": texts, "source": "auto", "target": target, "format": "text" }))?
            .send()
            .await
            .context("LibreTranslate isn't running at this address")?;
        anyhow::ensure!(resp.status().is_success(), "LibreTranslate: HTTP {}", resp.status());
        let v: Value = serde_json::from_slice(&resp.bytes().await?)?;
        let out: Vec<String> = v["translatedText"]
            .as_array()
            .context("LibreTranslate: unexpected reply")?
            .iter()
            .map(|t| t.as_str().unwrap_or_default().to_owned())
            .collect();
        anyhow::ensure!(out.len() == texts.len(), "LibreTranslate: missing translations");
        Ok(out)
    }
}

fn ai_name(id: &str) -> &'static str {
    match id {
        "gemini" => "Google Gemini",
        "groq" => "Groq",
        "openrouter" => "OpenRouter",
        "openai" => "OpenAI",
        "ollama" => "Ollama",
        _ => "AI server",
    }
}

/// Models installed in Ollama, if it's running
pub async fn ollama_models() -> anyhow::Result<Vec<String>> {
    let client = reqwest::Client::builder().timeout(Duration::from_secs(3)).build()?;
    let resp = client
        .get("http://127.0.0.1:11434/api/tags")
        .send()
        .await
        .context("Ollama isn't running")?;
    let v: Value = serde_json::from_slice(&resp.bytes().await?)?;
    Ok(v["models"]
        .as_array()
        .map(|m| m.iter().filter_map(|x| x["name"].as_str().map(str::to_owned)).collect())
        .unwrap_or_default())
}

trait JsonBody {
    fn json_body(self, v: &Value) -> anyhow::Result<reqwest::RequestBuilder>;
}

impl JsonBody for reqwest::RequestBuilder {
    fn json_body(self, v: &Value) -> anyhow::Result<reqwest::RequestBuilder> {
        Ok(self.header("Content-Type", "application/json").body(serde_json::to_vec(v)?))
    }
}

fn form_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}
