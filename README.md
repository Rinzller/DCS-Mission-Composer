# DCS Mission Composer

DCS Mission Composer is a Windows desktop app for collaborative DCS mission planning.

![DCS Mission Composer home screen](.github/media/app.png)

Mission designers can export planning copies from a `.miz`, let pilots or squadrons make their own mission-editor changes, review what changed, and merge approved planning work back into the main mission.

The goal is simple: keep mission ownership and secrecy intact while making realistic squadron planning easier.

## Features

- Validate dropped or selected `.miz` files as ZIP-based DCS mission archives.
- Export BLUE or RED coalition planning missions.
- Export individual client/player flights by airframe and group name.
- Embed a DCS Mission Composer manifest in exported planning missions.
- Compare an original mission against a modified planning mission.
- Summarize mission changes in a readable review panel.
- Detect merge-blocking warnings before writing output.
- Merge approved planning changes back into the original mission.
- Override blocked merges with an explicit confirmation flow.
- Preserve DTC and mission-editor changes in supported merge paths.
- Save merged/exported missions without overwriting loaded mission files.
- Show installed app version and check GitHub Releases for updates.
- Download the latest installer from the update prompt.
- Copy the local log file path for bug reports.
- Custom frameless window with minimize and close controls.

## Installation

1. Open the latest release:

   <https://github.com/Rinzller/DCS-Mission-Composer/releases/latest>

2. Download the Windows installer:

   ```text
   DCS-Mission-Composer_<version>_windows_x64-setup.exe
   ```

3. Run the installer.

4. Launch **DCS Mission Composer** from the Start Menu or installed shortcut.

Windows may show a SmartScreen warning for unsigned early builds. Choose **More info** and **Run anyway** only if you trust the release source.

## Basic Workflow

1. Load the main/original `.miz` in the left drop zone.
2. Choose an export scope:
   - Coalition export for all BLUE or RED planning assets.
   - Flight export for a detected client/player flight.
3. Export a planning `.miz` and send it to the planner or squadron.
4. Load the modified planning `.miz` in the right drop zone.
5. Review the change summary and merge status.
6. If the review is safe, click **MERGE** and choose an output `.miz`.
7. If the review is blocked, inspect warnings before using **Override merge**.

## Updates

The app displays its installed version in the bottom-right corner.

On startup, DCS Mission Composer checks GitHub Releases for the latest published version. If a newer version is available, the update badge changes to **Update available** and can download the latest installer.

Release page:

<https://github.com/Rinzller/DCS-Mission-Composer/releases>

## Logs

Use **Copy log path** in the app header to copy the local log file path. Include that log when reporting bugs.

The log records validation, export, compare, and merge operations, including success/failure status and diagnostic messages.

## Development

Prerequisites:

- Node.js
- npm
- Rust
- Visual Studio Build Tools with the Visual C++ workload

Install dependencies:

```powershell
npm install
```

Run checks:

```powershell
npm.cmd run check
```

Run the app in development:

```powershell
npm.cmd run tauri dev
```

Build the frontend:

```powershell
npm.cmd run build
```

Build the Windows installer:

```powershell
npm.cmd run tauri -- build --bundles nsis
```

If Cargo has trouble writing inside OneDrive, the local Tauri wrapper uses a temp target directory outside the repo. In CI, normal `src-tauri/target` output is preserved so GitHub Actions can find release artifacts.

## Tech Stack

- Tauri 2
- Rust
- Svelte 5
- TypeScript
- Vite
- npm

## Release

Releases are built by GitHub Actions when a version tag is pushed:

```powershell
git tag v0.1.1
git push origin v0.1.1
```

The release workflow builds the Windows NSIS installer, attaches it to the GitHub release, and uses GitHub-generated release notes.
