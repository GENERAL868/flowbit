# librqbit 9.0.1, patched for Flowbit

This folder is [librqbit](https://github.com/ikatson/rqbit) 9.0.1 by Igor Katson, as published on
crates.io, under the Apache License 2.0 (see `LICENSE`). Flowbit uses it through
`[patch.crates-io]` in `../../Cargo.toml`.

Changes from the published crate, each marked "Flowbit patch" in the code:

| File | Change |
|---|---|
| `src/safe_path.rs` | New. Turns names from torrent metadata into safe relative paths on Windows: no drive prefixes (`C:evil.exe`), alternate data streams, reserved names (`CON`, `NUL`…), forbidden characters, or trailing dots and spaces. |
| `src/lib.rs` | Exports `safe_path`. |
| `src/torrent_state/mod.rs` | File paths come from `safe_path::safe_relative_path`. |
| `src/session.rs` | Torrent subfolder names go through `safe_path` too. |
| `src/storage/filesystem/opened_file.rs` | `mark_file_sparse` was called inside `tracing::debug!()`, so it never ran with debug logging off; it now always runs, and files are sparse on Windows. Also `reopen_read_only()` and `lock_for_write()` (below). |
| `src/storage/mod.rs`, `src/storage/filesystem/fs.rs`, `src/storage/middleware/*.rs` | New `TorrentStorage::on_file_completed()`. The filesystem storage reopens a finished file read-only, and reopens it for writing if it's ever written to again. A read/write handle kept on Windows stops other programs (Explorer's thumbnails, players, antivirus) from opening the file with read-only sharing. |
| `src/torrent_state/live/mod.rs`, `src/torrent_state/initializing.rs` | Call `on_file_completed()` when a file's last piece is verified, and at startup for files that are already complete. Both places had a comment saying files are reopened read-only, but no code did it. |
