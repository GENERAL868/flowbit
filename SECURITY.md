# Security policy

## Supported versions

Only the latest release gets security fixes.

## Reporting a vulnerability

Please report it privately through GitHub:
[Security → Report a vulnerability](https://github.com/GENERAL868/flowbit/security/advisories/new).
Don't open a public issue.

Include what an attacker can do, the steps or a sample torrent / magnet link / web page that shows
it, and the Flowbit and Windows versions. You'll get an answer within a week, and credit in the
release notes if you want it.

## Scope

Flowbit handles untrusted input from torrents, magnet links, trackers, peers and websites. These
are especially interesting:

- Torrent file names that escape the download folder or write alternate data streams
- Websites reaching the local streaming server on `127.0.0.1`
- Requests to local services through trackers or URLs (SSRF)
- Running programs from torrents without the user choosing to
- HTML or script injection into the window through torrent or file names
- Leaking translation API keys

Bugs in librqbit itself that aren't specific to Flowbit can also go to
[rqbit](https://github.com/ikatson/rqbit), and mpv bugs to [mpv](https://github.com/mpv-player/mpv).

## What's already protected

What a malicious torrent, magnet link or website could do, and what stops it:

| Risk | Fix |
|---|---|
| A file named `C:evil.exe` in a torrent: on Windows `PathBuf::push` replaces the whole path, so a torrent could write, or on removal delete, files anywhere | `patches/librqbit/src/safe_path.rs`: every name is made a plain file name (no `:`, `<>"\|?*`, control characters, trailing dots/spaces, `CON`/`NUL`/`COM1`…), and paths must be relative. Also blocks alternate data streams (`file:stream`) |
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
