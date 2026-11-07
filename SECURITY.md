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

The protections already in place are listed under [Security](README.md#security) in the README.
Bugs in librqbit itself that aren't specific to Flowbit can also go to
[rqbit](https://github.com/ikatson/rqbit), and mpv bugs to [mpv](https://github.com/mpv-player/mpv).
