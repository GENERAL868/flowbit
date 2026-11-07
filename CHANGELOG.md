# Changelog

All notable changes to Flowbit. Versions follow [Semantic Versioning](https://semver.org).

## [1.3.0] - 2026-11-07

First public release.

### Torrents

- librqbit 9.0.1 engine in Rust: magnet links, `.torrent` files and URLs, DHT, UPnP, fast resume
- Stream video and audio in mpv while they download
- Choose which files to download; unchecking every file pauses the torrent and keeps what's on disk
- Share files and folders by dropping them on the window (asks first), or create a `.torrent`
- Download and upload limits, listening port, extra trackers
- Opens magnet links and `.torrent` files for the current user (no admin rights)
- Runs in the tray with only the engine (about 60 MB) when the window is closed

### Watch library

- Continue watching, episodes grouped by show, watched marks, "play next"
- Watch position followed through mpv's IPC pipe; marked watched at 90%
- Posters taken from the video with mpv

### Subtitle translator

- SRT, WebVTT and ASS; timings, styles and positions untouched; saved next to the video
- Engines: Google Translate, DeepL API Free, AI models (Gemini, Groq, OpenRouter, OpenAI, Ollama,
  any OpenAI-compatible server) and LibreTranslate, each with a setup guide in the app
- AI translation with context from earlier lines, Arabic style (MSA, Gulf, Egyptian, Levantine)
  and a glossary of names to keep
- Right-to-left fix and optional bilingual subtitles
- API keys encrypted with Windows DPAPI

### Interface

- English and Arabic (right to left), light and dark themes
- Pages stay centered on wide and maximized windows

### Security

- Safe file paths for torrent contents on Windows (drive prefixes, alternate data streams,
  reserved names)
- Locked-down local streaming server (secret URL, host check, GET/HEAD only)
- Loopback trackers stripped from magnet links
- Programs and scripts from torrents are revealed, not run; other downloads get the Mark of the Web
- The window can only navigate inside the app and only listen to events

[1.3.0]: https://github.com/GENERAL868/flowbit/releases/tag/v1.3.0
