# Flowbit

A streaming torrent client for Windows. Start watching while a torrent downloads, keep track of
what you've watched, and translate subtitles, in English or Arabic.

- **Torrent engine:** [librqbit](https://github.com/ikatson/rqbit) 9.0.1, in Rust
- **Window:** [Tauri](https://tauri.app) 2, using Windows' built-in WebView2
- **Player:** [mpv](https://mpv.io), bundled, in its own window with its own controls and shortcuts
  (ASS subtitles keep their styling)
- **UI:** plain JavaScript, no build step, in `ui/`; English and Arabic (right to left)

## Features

- Streams video and audio while it downloads; pick which files to download
- Watch library: continue watching, episodes grouped by show, watched marks, posters from the video
- Subtitle translator for SRT, WebVTT and ASS that keeps timings and styling: Google Translate,
  DeepL, AI models (Gemini, Groq, OpenRouter, OpenAI, Ollama or any OpenAI-compatible server) and
  LibreTranslate, with setup guides inside the app. API keys are encrypted with Windows DPAPI.
- Drag and drop `.torrent` files, magnet links and folders to share; create torrents
- Runs in the tray with only the engine (about 60 MB) when the window is closed
- Opens magnet links and `.torrent` files (current user, no admin rights)

## Layout

```
ui/                    the window: index.html, app.js, i18n.js (English, Arabic), style.css, app.css, fonts, sounds
src-tauri/src/
  lib.rs               app setup, commands for the window, tray, notifications, single instance
  engine.rs            librqbit session, torrent and file views, safe file paths
  stream_server.rs     the localhost streaming server mpv plays from
  player.rs            starts mpv on a stream URL
  library.rs           watch library (library.json), following playback through mpv's IPC pipe
  posters.rs           posters grabbed with mpv, and the Mark of the Web
  subtitles.rs         SRT/WebVTT/ASS parsing and writing, timings untouched
  translate.rs         translation engines
  keys.rs              API keys, encrypted with DPAPI
  media_cmds.rs        commands for the library, posters and the subtitle translator
  i18n.rs              tray, notification and dialog texts in English and Arabic
  drop.rs              files dropped on the window (paths through WebView2)
  prefs.rs             preferences (prefs.json)
  handlers.rs          registers magnet links and .torrent files
src-tauri/resources/   mpv's config, .torrent file icon (and mpv.exe, see below)
src-tauri/patches/     librqbit 9.0.1 with Windows fixes (see patches/librqbit/PATCHES.md)
```

## Building

You need Windows 10 or 11 with:

- [Rust](https://rustup.rs) (stable, `x86_64-pc-windows-msvc`) and the MSVC build tools
- [Node.js](https://nodejs.org) 20 or later
- `mpv.exe`: it's over GitHub's 100 MB file limit, so it isn't in the repository. Download a
  Windows x86_64 build from [shinchiro/mpv-winbuild-cmake](https://github.com/shinchiro/mpv-winbuild-cmake/releases)
  (Flowbit 1.3 ships release 20261018) and put `mpv.exe` in `src-tauri/resources/mpv/`.

```powershell
npm install
npx tauri build        # installer in src-tauri/target/release/bundle/nsis
```

For a portable copy, put `Flowbit.exe` and the `resources` folder from `src-tauri/target/release`
in a folder with an empty `Portable Settings` folder next to `Flowbit.exe`.

## Notes

- **Data:** `%LOCALAPPDATA%\app.flowbit.desktop` (prefs, session, DHT, library, WebView2 data). A
  `Portable Settings` folder next to `Flowbit.exe` makes it portable. `FLOWBIT_DATA_DIR`
  overrides both, for tests.
- **Streaming:** a small server (`src/stream_server.rs`) on `127.0.0.1:52525` (or a random port if
  taken). The fixed port and a secret kept in `stream-secret` keep stream URLs stable, so mpv
  remembers where you stopped watching.
- **TCP only:** librqbit marks uTP as experimental.
- **Not supported by librqbit:** web seeds (BEP 19) and WebRTC peers, so torrents that only have
  HTTP sources or only browser peers won't download.
- **Drag and drop:** Tauri's drop handling is off (`dragDropEnabled: false`), because the window
  under the cursor belongs to WebView2's browser process, where it can't register. WebView2 takes
  the drop, the page sends the files with `chrome.webview.postMessageWithAdditionalObjects`, and
  `src/drop.rs` reads their paths.

## Security

What a malicious torrent, magnet link or website could do, and what stops it:

| Risk | Fix |
|---|---|
| A file named `C:evil.exe` in a torrent: on Windows `PathBuf::push` replaces the whole path, so a torrent could write, or on removal delete, files anywhere | `patches/librqbit/src/safe_path.rs`: every name is made a plain file name (no `:`, `<>"|?*`, control characters, trailing dots/spaces, `CON`/`NUL`/`COM1`…), and paths must be relative. Also blocks alternate data streams (`file:stream`) |
| Websites reaching the localhost streaming server: probing or embedding your files, DNS rebinding | The server only streams, needs a 128-bit secret in the URL, accepts only `Host: 127.0.0.1`/`localhost`, GET/HEAD only, 8 KB of headers within 10 s |
| A magnet link with `tr=http://127.0.0.1:…` trackers making the app send requests to local services (SSRF) | Loopback trackers are removed from magnet links; `.torrent` URLs on loopback are refused |
| Clicking a `.exe`, `.bat`, `.lnk`, `.ps1`… from a torrent runs it | Programs and scripts are shown in their folder instead of being opened; other downloads get the Mark of the Web |
| Dropping a file by mistake shares it publicly at once | Sharing dropped files and folders asks first |
| Pasted text or the `add` command reading any local file | Only magnet links, http(s) links, info hashes, and `.torrent` files up to 20 MB |
| Torrent and file names in the window (HTML injection) | Everything from torrents is escaped; the CSP allows no inline scripts; the window may only listen to events and only navigate inside the app |
| API keys for translation services | Encrypted with Windows DPAPI for your account, sent only to their own service over HTTPS, never sent back to the window |

Dependencies: `cargo audit` reports `ringbuf` 0.4.8 (RUSTSEC-2026-0293) through librqbit's uTP code.
It needs an element whose `Drop` panics; uTP only stores bytes, and uTP is off here, so it can't
happen.

## License

MIT, see [LICENSE](LICENSE). Third-party parts keep their own licenses: librqbit (Apache-2.0),
mpv (GPL-2.0-or-later) and the fonts (SIL OFL 1.1). See
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
