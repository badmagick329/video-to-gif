# Releasing

A v0 release is one unsigned NSIS installer, built locally with `pnpm tauri build` and handed out by hand. Code signing, auto-update and CI are later work ([v0 scope](../decisions/2026-11-24-v0-scope.md)); until signing lands, SmartScreen warns on first run and the root [README](../../README.md) tells users to click *More info*, then *Run anyway*.

## Bumping the version

The version lives in three files and all three must match:

- `version` in [package.json](../../package.json)
- `version` under `[package]` in [src-tauri/Cargo.toml](../../src-tauri/Cargo.toml); the next cargo build rewrites `Cargo.lock`, so commit that too
- `version` in [src-tauri/tauri.conf.json](../../src-tauri/tauri.conf.json): the installer's filename, its Add/Remove Programs entry and the exe's version resource read this one

## Building the installer

`pnpm tauri build` runs `pnpm build` (type check and Vite), compiles the release exe and wraps it in an NSIS installer. The first run downloads the NSIS toolchain into Tauri's cache. Output:

- Installer: `src-tauri/target/release/bundle/nsis/video-to-gif_<version>_x64-setup.exe`
- Bare exe: `src-tauri/target/release/video-to-gif.exe`

Before handing the installer out, confirm the filename carries the new version and the exe reports it: `(Get-Item src-tauri/target/release/video-to-gif.exe).VersionInfo.ProductVersion`.

The bundle settings in `tauri.conf.json` shape the user's install:

- `targets: ["nsis"]`: an MSI would need WiX and per-machine installs; one installer type keeps the README's install steps single.
- `nsis.installMode: "currentUser"`: installs into `%LOCALAPPDATA%\video-to-gif` with a Start menu shortcut and an HKCU uninstall entry, so the installer never asks for administrator rights.
- `webviewInstallMode: downloadBootstrapper` (silent): Windows 11 and current Windows 10 already ship WebView2, so the installer stays small and fetches the bootstrapper only on a PC that lacks it.

## Regenerating the icon

The source is [src-tauri/icons/icon.svg](../../src-tauri/icons/icon.svg): a primary-dg orange loop arrow around a light play button on a secondary-dg tile (the `-dg` colours in [tailwind.config.js](../../tailwind.config.js) and `src/index.css`). Edit the SVG, then run `pnpm icon`. It runs `tauri icon` on the SVG, which rasterises it itself and rewrites every PNG, `icon.ico` and `icon.icns` in `src-tauri/icons/`, then deletes the `android/` and `ios/` sets it also generates, which this Windows-first app does not ship.

Check the 16 px layer of `icon.ico` before committing: it is what the taskbar's small views, Explorer's list view and the window's title bar show. Shapes stay thick there only if strokes stay near 100 units of the 1024 canvas, and the play button stays light so it separates from the orange loop.
