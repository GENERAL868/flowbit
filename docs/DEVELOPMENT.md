# Development

How the code is laid out, how to build Flowbit, and notes on how it works. To contribute, see also [CONTRIBUTING.md](../CONTRIBUTING.md).

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
  keys.rs              API keys (DPAPI on Windows, a private file on Linux)
  media_cmds.rs        commands for the library, posters and the subtitle translator
  i18n.rs              tray, notification and dialog texts in English and Arabic
  drop.rs              files dropped on the window (paths through WebView2, Windows)
  prefs.rs             preferences (prefs.json)
  handlers.rs          registers magnet links and .torrent files (Windows registry)
  handlers_linux.rs    the same on Linux (desktop entry + xdg-mime)
src-tauri/resources/   mpv's config, .torrent file icon (and mpv.exe, see below)
src-tauri/tauri.linux.conf.json   Linux packages: deb, rpm, AppImage
src-tauri/patches/     librqbit 9.0.1 with Windows fixes (see patches/librqbit/PATCHES.md)
```

## Building

### Linux

On Ubuntu or Debian, [`.github/scripts/linux-deps.sh`](../.github/scripts/linux-deps.sh) installs what's needed (WebKitGTK 4.1, the tray library…). Then, with [Rust](https://rustup.rs) and [Node.js](https://nodejs.org) 20 or later:

```bash
npm install
npx tauri build        # .deb, .rpm and .AppImage in src-tauri/target/release/bundle
```

Running needs `mpv` (and `xdg-utils`, for magnet links). Releases build the Linux packages on GitHub Actions ([`release.yml`](../.github/workflows/release.yml)), on Ubuntu 22.04 (glibc 2.35, WebKitGTK 4.1).

### Windows

You need Windows 10 or 11 with:

- [Rust](https://rustup.rs) (stable, `x86_64-pc-windows-msvc`) and the MSVC build tools
- [Node.js](https://nodejs.org) 20 or later
- `mpv.exe`: it's over GitHub's 100 MB file limit, so it isn't in the repository. Download a Windows x86_64 build from [shinchiro/mpv-winbuild-cmake](https://github.com/shinchiro/mpv-winbuild-cmake/releases) (Flowbit ships release 20261018) and put `mpv.exe` in `src-tauri/resources/mpv/`.

```powershell
npm install
npx tauri build        # installer in src-tauri/target/release/bundle/nsis
```

For a portable copy, put `Flowbit.exe` and the `resources` folder from `src-tauri/target/release` in a folder with an empty `Portable Settings` folder next to `Flowbit.exe`.

## Notes

- **Stack:** librqbit 9.0.1 for torrents; Tauri 2 on the system's webview (WebView2 on Windows, WebKitGTK on Linux); mpv for playback, bundled on Windows and the system's on Linux; plain JavaScript in `ui/`, with no build step.
- **Data:** `%LOCALAPPDATA%\app.flowbit.desktop` on Windows, `~/.local/share/app.flowbit.desktop` on Linux (prefs, session, DHT, library, webview data). A `Portable Settings` folder next to `Flowbit.exe` makes it portable. `FLOWBIT_DATA_DIR` overrides both, for tests.
- **Streaming:** a small server (`src/stream_server.rs`) on `127.0.0.1:52525` (or a random port if taken). The fixed port and a secret kept in `stream-secret` keep stream URLs stable, so mpv remembers where you stopped watching.
- **TCP only:** librqbit marks uTP as experimental.
- **Not supported by librqbit:** web seeds (BEP 19) and WebRTC peers, so torrents that only have HTTP sources or only browser peers won't download.
- **Drag and drop:** Tauri's drop handling is off (`dragDropEnabled: false`), because the window under the cursor belongs to WebView2's browser process, where it can't register. WebView2 takes the drop, the page sends the files with `chrome.webview.postMessageWithAdditionalObjects`, and `src/drop.rs` reads their paths.
