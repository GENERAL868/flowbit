<p align="center">
  <img src="assets/icon.png" width="96" height="96" alt="">
</p>

<h1 align="center">Flowbit</h1>

<p align="center">
  A streaming torrent client for Windows and Linux. Start watching while it downloads, keep track
  of what you've watched, and translate subtitles, in English or Arabic.
</p>

<p align="center">
  <a href="https://github.com/GENERAL868/flowbit/releases/latest"><img src="https://img.shields.io/github/v/release/GENERAL868/flowbit?label=download&color=3DDC97&cacheSeconds=600" alt="Latest release"></a>
  <a href="https://github.com/GENERAL868/flowbit/actions/workflows/ci.yml"><img src="https://github.com/GENERAL868/flowbit/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <img src="https://img.shields.io/badge/platform-Windows%20%7C%20Linux-blue" alt="Windows and Linux">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue" alt="MIT license"></a>
</p>

<p align="center">
  <img src="docs/screenshots/home-en.png" width="860" alt="Flowbit's torrent list, with a download in progress">
</p>

## Download

Get the latest version from [Releases](https://github.com/GENERAL868/flowbit/releases/latest).

### Windows 10 and 11 (64-bit)

- **`Flowbit-x.y.z-setup.exe`**: installs for your account only, no admin rights needed
- **`Flowbit-x.y.z-portable.zip`**: unzip anywhere and run `Flowbit.exe`; settings stay in the
  folder

Uses Microsoft Edge WebView2, which Windows already has. mpv comes with it.

### Linux (64-bit)

Flowbit plays videos in your system's **mpv**; the packages install it for you.

| Distribution | File | Install |
|---|---|---|
| Ubuntu, Debian, Mint, Pop!_OS… | `Flowbit-x.y.z-linux-amd64.deb` | `sudo apt install ./Flowbit-x.y.z-linux-amd64.deb` |
| Fedora, openSUSE, RHEL… | `Flowbit-x.y.z-linux-x86_64.rpm` | `sudo dnf install ./Flowbit-x.y.z-linux-x86_64.rpm` |
| Any other | `Flowbit-x.y.z-linux-x86_64.AppImage` | install `mpv`, then `chmod +x Flowbit-*.AppImage` and run it |

Built on Ubuntu 22.04, so it runs on distributions from 2022 on (glibc 2.35, WebKitGTK 4.1).

## Built with

- **Torrent engine:** [librqbit](https://github.com/ikatson/rqbit) 9.0.1, in Rust
- **Window:** [Tauri](https://tauri.app) 2, using the system's webview (WebView2 on Windows,
  WebKitGTK on Linux)
- **Player:** [mpv](https://mpv.io) (bundled on Windows, the system's on Linux), in its own window
  with its own controls and shortcuts (ASS subtitles keep their styling)
- **UI:** plain JavaScript, no build step, in `ui/`; English and Arabic (right to left)

## Features

- Streams video and audio while it downloads; pick which files to download
- Watch library: continue watching, episodes grouped by show, watched marks, posters from the video
- Subtitle translator for SRT, WebVTT and ASS that keeps timings and styling: Google Translate,
  DeepL, AI models (Gemini, Groq, OpenRouter, OpenAI, Ollama or any OpenAI-compatible server) and
  LibreTranslate, with setup guides inside the app. API keys are encrypted with Windows DPAPI
  (on Linux, kept in a file only your account can read).
- Drag and drop `.torrent` files and magnet links (and, on Windows, files and folders to share);
  create torrents
- Runs in the tray with only the engine (about 60 MB) when the window is closed
- Opens magnet links and `.torrent` files (current user, no admin rights)

| Watch library | Subtitle translator |
|---|---|
| <img src="docs/screenshots/library-en.png" alt="Watch library with continue watching and episodes"> | <img src="docs/screenshots/translator-en.png" alt="Subtitle translator with four engines"> |

<details dir="rtl">
<summary><b>بالعربي</b></summary>

**Flowbit** برنامج تورنت لويندوز ولينكس، تشاهد فيه الحلقة وهي تنزل.

- **مكتبة مشاهدة:** تكمل من حيث وقفت، والحلقات مرتبة حسب المسلسل، وتعلّم على اللي شفته.
- **مترجم ترجمات:** يترجم ملفات SRT و ASS و WebVTT للعربي ويحافظ على التوقيت والتنسيق. يشتغل مع Google و DeepL ونماذج الذكاء الاصطناعي المجانية (Gemini و Groq وغيرها)، وتختار أسلوب العربي: فصحى أو خليجي أو مصري أو شامي.
- **واجهة عربية كاملة** من اليمين لليسار.
- **مجاني ومفتوح المصدر.**

نزّله من صفحة [الإصدارات](https://github.com/GENERAL868/flowbit/releases/latest).

<img src="docs/screenshots/library-ar.png" alt="مكتبة المشاهدة بالعربي">

</details>

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

On Ubuntu or Debian, [`.github/scripts/linux-deps.sh`](.github/scripts/linux-deps.sh) installs
what's needed (WebKitGTK 4.1, the tray library…). Then, with [Rust](https://rustup.rs) and
[Node.js](https://nodejs.org) 20 or later:

```bash
npm install
npx tauri build        # .deb, .rpm and .AppImage in src-tauri/target/release/bundle
```

Running needs `mpv` (and `xdg-utils`, for magnet links). Releases build the Linux packages on
GitHub Actions ([`release.yml`](.github/workflows/release.yml)).

### Windows

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

- **Data:** `%LOCALAPPDATA%\app.flowbit.desktop` on Windows, `~/.local/share/app.flowbit.desktop`
  on Linux (prefs, session, DHT, library, webview data). A `Portable Settings` folder next to
  `Flowbit.exe` makes it portable. `FLOWBIT_DATA_DIR` overrides both, for tests.
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
| Clicking a `.exe`, `.bat`, `.lnk`, `.ps1`… (on Linux an executable, `.desktop` or `.sh` file) from a torrent runs it | Programs and scripts are shown in their folder instead of being opened; on Windows other downloads get the Mark of the Web |
| Dropping a file by mistake shares it publicly at once | Sharing dropped files and folders asks first |
| Pasted text or the `add` command reading any local file | Only magnet links, http(s) links, info hashes, and `.torrent` files up to 20 MB |
| Torrent and file names in the window (HTML injection) | Everything from torrents is escaped; the CSP allows no inline scripts; the window may only listen to events and only navigate inside the app |
| API keys for translation services | Encrypted with Windows DPAPI for your account (on Linux, a file with mode 0600), sent only to their own service over HTTPS, never sent back to the window |

Dependencies: `cargo audit` reports `ringbuf` 0.4.8 (RUSTSEC-2026-0293) through librqbit's uTP code.
It needs an element whose `Drop` panics; uTP only stores bytes, and uTP is off here, so it can't
happen.

## Contributing

Bug reports, translations and pull requests are welcome: see [CONTRIBUTING.md](CONTRIBUTING.md)
and the [code of conduct](CODE_OF_CONDUCT.md).
Report security problems privately, as described in [SECURITY.md](SECURITY.md). Changes are listed
in [CHANGELOG.md](CHANGELOG.md).

## License

MIT, see [LICENSE](LICENSE). Third-party parts keep their own licenses: librqbit (Apache-2.0),
mpv (GPL-2.0-or-later) and the fonts (SIL OFL 1.1). See
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
