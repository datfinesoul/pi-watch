# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.6.0] - 2026-11-11

### Added

- Show blocking extension dialogs as input requests through Pi's native `ui_prompt_start` and `ui_prompt_end` events. This includes `pi-questions` without tool-specific integration.
- Show separate input and completion counts in the in-terminal widget. Prioritize input requests when cycling Kitty panes.
- Test native prompt-event spans through installed Pi, including overlapping dialogs and failure cleanup.

### Changed

- Keep input waiting independent of agent activity and completion acknowledgment. Visiting a pane does not clear an open question.
- Require Pi 1.5.2 or newer. Keep older completion-state files readable and add an optional `waiting` flag.
- Replace runtime-state files atomically and remove input handlers during shutdown or session replacement.
- Use the extension-only module profile and Node CI. Keep completion alerts and the local Ctrl+' shortcut on macOS and Linux.

### Removed

- Remove the native macOS app, Rust sources, app build scripts, native checks, and menu bar shortcut.
- Remove app IPC, assistant-response previews, Git project labels, and pane-title collection. Existing installed apps and build output require manual removal.

## [0.5.1] - 2026-11-10

### Fixed

- Use TypeScript runtime helpers so `/reload` refreshes imported logic instead of retaining cached JavaScript helpers. Keep the companion app behavior and IPC protocol unchanged.

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

[Unreleased]: https://github.com/datfinesoul/pi-watch/compare/v0.6.0...HEAD
[0.6.0]: https://github.com/datfinesoul/pi-watch/compare/v0.5.1...v0.6.0
[0.5.1]: https://github.com/datfinesoul/pi-watch/compare/v0.5.0...v0.5.1
[0.5.0]: https://github.com/datfinesoul/pi-watch/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/datfinesoul/pi-watch/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/datfinesoul/pi-watch/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/datfinesoul/pi-watch/releases/tag/v0.2.0
