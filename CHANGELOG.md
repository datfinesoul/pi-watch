# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.5.0] - 2026-11-03

### Added

- Cross-session pending awareness: each reporting Pi session publishes a one-line state file under `$XDG_STATE_HOME/pi-watch/sessions/`. The file holds the version, the pid, the pending flag, and the Kitty window id. A session removes its file on shutdown, and a pid-liveness probe sweeps stale files. A reporting session shows `◇ N done elsewhere — ctrl+' switch` above the editor while N other sessions await inspection. The extension consumes ctrl+' (legacy BEL and Kitty-protocol CSI forms) only while other sessions await, and focuses the next one with `kitty @ focus-window`. This is extension-local: no notifications, popups, global grabs, or report changes.
- A session clears its pending state on a new prompt, on shutdown, or when a heartbeat sees its pane change from unfocused to focused. A pane that was already focused when its run ended stays pending until you leave and return.

## [0.4.0] - 2026-10-28

### Added

- Response previews in session submenus, accessible with Right Arrow. Previews contain bounded beginning and ending fragments and stay in memory only.
- Optional debug diagnostics for report delivery and preview availability, without response text.

### Changed

- Response submenus show only preview text, without a heading or navigation action.

## [0.3.0] - 2026-10-28

### Added

- Global Cmd+Shift+' shortcut to open the menu, with plain digits 1–9 for navigable unseen completions.
- Optional `PI_WATCH_DEBUG=1` stderr logging for shortcut delivery and menu tracking, without session content.
- A completion section before the project groups. Menu rows and navigation targets remain fixed while the menu is open.

### Changed

- The menu bar shows `π N` for unseen completions, `π ●` for working sessions without unseen completions, and `π` otherwise.
- Idle session details remain in the dropdown instead of menu bar statistics.

### Fixed

- Repeated shortcut presses no longer open nested menus that each require Escape to dismiss.

## [0.2.0] - 2026-10-28

### Added

- Standard Pi package manifest, license, community files, contribution guidance, and security documentation.
- Node tests, package-content checks, shared-version validation, and an optional installed-Pi loader check.
- Node CI, macOS native CI, and the source-only version-tagging workflow.
- Explicit source-build, installation, removal, compatibility, and manual desktop-check instructions.

### Changed

- Adopted the control plane's extension-with-companion-app standard and removed the experimental classification.
- Moved the reporting entry point to `extensions/watch/index.ts`, extension logic to root `src/`, and Node tests to root `test/`.
- Moved native sources, Cargo files, and app resources under `apps/macos/`, retaining root Make commands and output directories.
- Aligned extension, Cargo, lockfile, and bundle versions at `0.2.0` without changing IPC protocol 1 or current session-monitoring behavior.
- Added native formatting, linting, tests, and bundle checks to full control-plane readiness on macOS.

[Unreleased]: https://github.com/datfinesoul/pi-watch/compare/v0.4.0...HEAD
[0.4.0]: https://github.com/datfinesoul/pi-watch/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/datfinesoul/pi-watch/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/datfinesoul/pi-watch/releases/tag/v0.2.0
