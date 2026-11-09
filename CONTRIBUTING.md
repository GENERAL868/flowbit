# Contributing to Flowbit

Thanks for helping. Bug reports, translations and pull requests are all welcome.

## Reporting bugs and ideas

Open an [issue](https://github.com/GENERAL868/flowbit/issues/new/choose) and pick a template.
For bugs, include your Flowbit and Windows versions and the steps that show the problem. Please
don't attach torrents of copyrighted material.

Security problems go through [private reporting](SECURITY.md), not public issues.

## Building

See [Building](docs/DEVELOPMENT.md#building). In short: Rust (MSVC), Node.js 20+, `mpv.exe` in
`src-tauri/resources/mpv/`, then `npm install` and `npx tauri dev` for a debug window or
`npx tauri build` for the installer.

For tests and trying things out without touching your own data, point Flowbit at a scratch
folder:

```powershell
$env:FLOWBIT_DATA_DIR = "$env:TEMP\flowbit-dev"
npx tauri dev
```

## Pull requests

- Keep each pull request to one change, and describe what it fixes and how you checked it.
- Run the checks before you push:

  ```powershell
  cd src-tauri
  cargo fmt --check
  cargo test
  ```

- Match the style around you: short functions, comments that explain why, no new dependencies
  unless they earn their place.
- UI text goes in `ui/i18n.js` (window) or `src-tauri/src/i18n.rs` (tray, notifications, dialogs),
  in **both English and Arabic**. If you can't write the Arabic, say so in the pull request and
  someone will add it.
- The UI is plain JavaScript with no build step; please keep it that way.
- Changes to `src-tauri/patches/librqbit` must stay marked "Flowbit patch" and be listed in
  `patches/librqbit/PATCHES.md` (the Apache-2.0 license requires it).

## Translations

New languages are welcome: add a block to `STRINGS` in `ui/i18n.js` and the matching texts in
`src-tauri/src/i18n.rs`.

## License

By contributing, you agree that your contributions are licensed under the [MIT License](LICENSE).
