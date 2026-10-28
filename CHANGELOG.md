# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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

[Unreleased]: https://github.com/datfinesoul/pi-watch/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/datfinesoul/pi-watch/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/datfinesoul/pi-watch/releases/tag/v0.2.0
