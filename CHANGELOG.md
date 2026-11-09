# Changelog

All notable changes to Flowbit. Versions follow [Semantic Versioning](https://semver.org).

## 0.1.0 - Unreleased

Version numbers start over at 0.1.0 with this release. Releases 1.3.0 to 1.4.0 below came before it.

### Added

- **Select all**: tick torrents in the list, or all of them, and remove them together, with or
  without their files.

### Fixed

- Reopening Flowbit no longer repeats the "Download finished" notification for torrents that
  finished earlier.

## [1.4.0] - 2026-11-07

### Added

- **Linux**: `.deb` (Ubuntu, Debian…), `.rpm` (Fedora, openSUSE…) and AppImage packages, built on
  Ubuntu 22.04. Everything works as on Windows, with these differences:
  - Videos play in the system's mpv (the packages install it). Your own mpv settings are kept.
  - Flowbit makes itself the app for magnet links and `.torrent` files with `xdg-mime`.
  - API keys are kept in a file only your account can read, as Linux has no built-in equivalent
    of Windows DPAPI.
  - Dropping `.torrent` files and magnet links works; to share your own files, use **Add torrent
    → Share a folder or a file**.
  - Executable files, `.desktop` launchers and scripts from torrents are shown in their folder
    instead of being opened.

## [1.3.1] - 2026-11-07

### Added

- **Stop when a download finishes** (Preferences → Downloads, on by default): finished torrents
  pause instead of seeding. Press **Start seeding** on a torrent to keep sharing it, and Flowbit
  remembers that choice. Files and folders you share yourself always seed.

### Changed

- Playing a file that's already downloaded no longer restarts the torrent: it plays from disk.
- "Resume all" (window and tray) leaves finished torrents stopped.

### Fixed

- Explorer shows thumbnails for finished videos while the torrent is still in the list. Finished
  files used to stay open for writing, which kept other programs from reading them.

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

[1.4.0]: https://github.com/GENERAL868/flowbit/releases/tag/v1.4.0
[1.3.1]: https://github.com/GENERAL868/flowbit/releases/tag/v1.3.1
[1.3.0]: https://github.com/GENERAL868/flowbit/releases/tag/v1.3.0
