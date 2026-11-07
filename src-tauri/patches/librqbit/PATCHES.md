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
| `src/storage/filesystem/opened_file.rs` | `mark_file_sparse` was called inside `tracing::debug!()`, so it never ran with debug logging off; it now always runs, and files are sparse on Windows. |
