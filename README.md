# pi-watch

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

A [Pi](https://github.com/earendil-works/pi) reporting extension with a native macOS menu bar companion app. Pi Watch shows active and idle sessions across Kitty panes, groups them by project, and lets you return to a pane. It uses Rust and AppKit, not a webview, transcript scanner, or model calls.

## Installation

### Global installation

Install the reporting extension for every project. Pi records the package in `~/.pi/agent/settings.json`:

```sh
pi install git:github.com/datfinesoul/pi-watch
```

### Project installation

Install the reporting extension for the current project. Pi records the package in `.pi/settings.json`:

```sh
pi install --local git:github.com/datfinesoul/pi-watch
```

Project packages load only after Pi grants project trust. Run `/reload` in existing Pi sessions or start new sessions after installation. Reporting requires Pi 1.4.0 or newer with `agent_settled` and Node.js 22.19 or newer.

> [!NOTE]
> Pi identifies a package by where it was installed from. Installing pi-watch globally and for a project from the same source is fine: the project installation replaces the global one. Installing it from different locations, such as GitHub globally and a local checkout for a project, loads both copies, and Pi does not reconcile them.

<details>
<summary>Install from a local checkout</summary>

Install globally:

```sh
pi install /path/to/pi-watch
```

Or install for the current project:

```sh
pi install --local /path/to/pi-watch
```

From the checkout, try reporting without changing Pi settings:

```sh
pi --no-extensions --extension ./extensions/watch/index.ts
```

Use an absolute extension path outside the checkout. The control plane already loads this working tree and disables the global Git package. Do not re-enable the global copy here.

> The former entry point was `extensions/pi-watch.ts`. Update explicit `--extension` paths and restart those sessions. Remove or relocate any manually installed old extension copy to avoid duplicate reporters. Old instructions used the `making-it-work` Git branch. Choose the unpinned Git source above when you are ready to follow the default branch. The native app now lives under `apps/macos/`, but root Make commands remain available.

</details>

### Companion app

Pi installation loads only the reporting extension. It does not build, install, or start the app. A stopped or missing app does not prevent Pi from working.

The app currently supports macOS. Kitty is required for pane navigation. Linux support and prebuilt downloads are deferred. Build the app from a repository checkout with Node.js 22.19+, Rust 1.99.0+, and Apple command-line build tools. CI uses Rust 1.99.0 and `macos-15`. The same Rust sources serve Intel and Apple Silicon, but each build targets its host architecture. No universal binary or cross-architecture distribution is provided.

```sh
git clone https://github.com/datfinesoul/pi-watch.git
cd pi-watch
make build
```

This creates `dist/Pi Watch.app` with a local ad-hoc signature. It is not Developer ID signed or notarized. To run it from the checkout:

```sh
make open
```

To install a copy for your user, quit the running app before replacing it:

```sh
mkdir -p "$HOME/Applications"
ditto "dist/Pi Watch.app" "$HOME/Applications/Pi Watch.app"
open "$HOME/Applications/Pi Watch.app"
```

App installation and startup are explicit steps. No launch-at-login entry or background service is installed. A later full release will provide prebuilt apps after distribution and signing requirements are settled.

To remove an installed app, quit Pi Watch and remove `~/Applications/Pi Watch.app`. Remove a checkout build by removing `dist/Pi Watch.app`. To remove the extension, use the source you installed, with `--local` for project installations:

```sh
pi remove git:github.com/datfinesoul/pi-watch
# Or, from the project where it was installed:
pi remove --local git:github.com/datfinesoul/pi-watch
```

For local installations, substitute `/path/to/pi-watch` as the source. Run `/reload` in affected sessions. Project removal leaves any global installation intact. A command-line `--extension` remains active until you restart that Pi process without the flag.

## Usage

Start the app, then load the reporting extension in each Pi session you want to monitor. Sessions without Kitty still report status, but pane navigation is unavailable.

The menu bar prioritizes completed sessions awaiting inspection. It shows `π N` when unseen active-to-idle transitions exist, even if other sessions remain active. Otherwise, it shows `π ●` while sessions work, or `π` when all sessions are idle. Disconnected sessions do not affect these indicators.

Open the menu for a **Done — awaiting inspection** section, followed by remaining sessions grouped by working directory. Rows show status duration and Kitty pane titles. Git labels use forms such as `↳/superproject/repo/src` or `repo/src`. Non-Git directories use their directory name, and pane numbers replace unavailable titles. Process IDs do not appear in the interface.

Click a session to focus its Kitty pane and activate Kitty on macOS. Rows without a pane ID or socket endpoint remain disabled. Successful navigation clears that session's unseen completion. Opening the menu alone does not clear completions. Manual navigation clears a completion when Kitty reports focus, normally within five seconds. Failed navigation leaves the completion pending.

Configure Kitty remote control outside Pi. Start reporting Pi processes inside Kitty with `KITTY_WINDOW_ID` and a Unix `KITTY_LISTEN_ON` endpoint. Prefer socket-only access in a user-owned directory with mode `0700`. Pi Watch does not configure Kitty or discover another endpoint. The app searches `/Applications/kitty.app` and `~/Applications/kitty.app`. Command failures appear on standard error.

Use **Quit Pi Watch** to exit. Only one app instance can own the reporting socket. Menu content refreshes once per second during the normal app run loop. Close and reopen a menu that stays open across a status change.

Popovers remain disabled by default behind the Cargo feature `popovers`. To try them, stop the bundled app, then run:

```sh
CARGO_TARGET_DIR="$PWD/target" cargo run --manifest-path apps/macos/Cargo.toml --features popovers
```

Popovers appear after active-to-idle transitions and close after about five seconds or an outside click. Completions observed together share one popover. The standard bundle uses default features. Notification banners, launch-at-login, persistent history, custom icons, and needs-attention inference are not included.

## Keyboard shortcut

Press **Cmd+Shift+'** to open the menu from another app. Release the modifiers, then press **1–9** to activate a numbered completion. Only the first nine navigable completions receive numbers. Other rows remain accessible with the mouse or arrow keys. Press **Escape** to dismiss the menu.

Menu rows and numbers remain fixed while the menu is open. Close and reopen it to see new completions. The shortcut uses the physical ANSI quote key, which can have another label on non-US layouts. It uses macOS hotkey registration, not a keyboard monitor, and requires no Input Monitoring permission. If registration fails, the app logs the macOS error to standard error. Mouse navigation remains available.

## How it works

### Lifecycle and markers

Reporting starts with Pi's current idle state. `before_agent_start` and `agent_start` mark processing as active. Only `agent_settled` returns it to idle. `agent_end` is not final because retries and queued work can follow it. Idle means automatic processing stopped, not that an approval dialog appeared or a task succeeded.

A five-second heartbeat restores reporting after an app restart. The app hides sessions after a disconnect report or 15 seconds without a report. Registry entries expire after five minutes. The app removes expired entries when another report arrives. Durations start when the app observes a status change. A new reporting instance replaces the entry from the same Pi process, avoiding duplicate rows after reload or session replacement.

Unknown focus does not clear a marker. A focused pane can briefly show one until the next report. Visits between polls can be missed. A new active run, disconnect, stale reconnect, or reporting-instance replacement resets its marker. App restarts discard all in-memory state. Startup and repeated idle reports do not create markers.

### Privacy and runtime paths

Reports contain protocol version, random reporting-instance ID, working directory, project label, process ID, and working/idle/disconnected status. Kitty metadata includes pane ID, title, control socket endpoint, and focus state. A bounded Kitty listing request selects the reporting pane and retains only its title and focus flags. The extension discards other listing fields without logging or sending them. Avoid sensitive pane titles because terminal applications can set them.

Highlight a session and press Right Arrow to open its submenu. The submenu shows beginning and ending fragments of the last completed assistant response. Use Left Arrow to return. The submenu contains only the preview, with an ellipsis between fragments. Sessions without a preview have no submenu. Previews appear only for responses completed after the extension loads. The previous preview remains visible during the next response.

The extension extracts at most 240 Unicode characters from each end of completed assistant text. It sends only these fragments through local IPC. Previews stay in memory and are not logged or persisted. The extension does not inspect stored transcripts, prompts, thinking content, tool arguments, tool output, or credentials. It makes no model calls. Reports remain in app memory, and there is no network listener. The extension uses `git` and `kitty` from Pi's `PATH`. Missing commands produce fallback labels and unknown focus rather than stopping status reporting.

IPC uses `~/Library/Application Support/Pi Watch/events.sock` in a user-owned directory with mode `0700`. The socket uses mode `0600`. The app limits frames to 8 KiB and registry entries to 512. Clients have a short read timeout. An `app.lock` file prevents concurrent app instances from replacing each other's sockets. The socket may remain after exit. The app replaces it on startup. These are native app runtime paths, not extension configuration layers. No user configuration files are currently read.

Any process running as the same user can submit status. Kitty remote control permits more than pane focusing. Pi Watch is not a security boundary against malicious software under your account. See [SECURITY.md](SECURITY.md).

### Compatibility

The extension and app share one product version. The IPC protocol has its own version, currently `1`. This standardization keeps protocol 1 and existing behavior unchanged. The receiver rejects other protocol versions and unknown fields. For a future protocol change, upgrade and restart the app first, then reload reporting extensions. Mixed incompatible protocol versions will not report correctly.

## Development

The extension requires no runtime dependencies beyond the Pi host and has no build step. The app has separate Cargo dependencies and a locked native build. The extension's npm package excludes native sources, binaries, and development scripts. Obtain app sources from this repository rather than the npm package.

Run the extension tests and inspect the package with npm or pnpm:

```sh
npm test
npm run pack:check
```

Use `pnpm test` and `pnpm run pack:check` with pnpm. The tests cover metadata parsing, lifecycle events, privacy fields, missing-app recovery, heartbeat cleanup, package contents, and shared versions. Set `PI_CODING_AGENT_ENTRY` to an installed Pi `dist/index.js` to also test the packaged entry point through Pi's loader. That optional test makes no model requests and does not start the app or reporting timers.

Run all native deterministic checks on macOS:

```sh
npm run check:app
```

This runs Rust formatting, Clippy, tests for default and optional features, and a release bundle build with signature verification. Missing required tools fail the check. It never launches or restarts the app. `make check` runs extension tests, package checks, and native checks together. The control plane's full readiness invokes these checks on macOS and warns about incomplete native validation elsewhere.

Use the root Makefile for app development:

```sh
make build    # Build without starting the app
make open     # Open the checkout build
make stop     # Stop the running app
make restart  # Rebuild, stop, and reopen the checkout build
make restart-debug  # Rebuild and restart with foreground debug logs
```

From the control plane, use `make -C pi-watch <target>`. App-only changes require an app restart. Extension changes require `/reload` in each reporting Pi session. If the protocol changes, restart the updated app before reloading extensions.

### Shortcut debug logging

Use `make restart-debug` to rebuild, stop the running app, and start a foreground instance with `PI_WATCH_DEBUG=1`. Debug output appears in the terminal and in `target/pi-watch-debug.log`. Each run overwrites the previous log. Logging is off for normal app starts. From the control-plane root:

```sh
make -C pi-watch restart-debug
# Optional log destination:
make -C pi-watch restart-debug DEBUG_LOG=/tmp/pi-watch-debug.log
```

Reproduce the problem, then quit Pi Watch through its menu. The log includes the process ID, relative times, Carbon event timestamps, dismissal cutoff, callback guards, and menu delegate events. It also records report receipt, report rejection categories, preview presence, fragment character counts, and preview state when the menu opens. It excludes session titles, paths, prompts, and response text. Remove the log when it is no longer needed. Start the app normally without the environment variable to disable logging.

### Manual desktop checks

Automated checks do not check AppKit placement, desktop focus behavior, or live Kitty permissions. Check these in a logged-in macOS desktop session after related changes:

- [ ] The menu bar shows `π N` for unseen completions, `π ●` for working sessions without unseen completions, and `π` otherwise.
- [ ] An off-pane session that becomes idle enters the Done section. Visiting it through Kitty or Pi Watch clears its completion.
- [ ] Cmd+Shift+' opens the menu from another app. Plain digits 1–9 activate the corresponding numbered panes, and Escape dismisses the menu.
- [ ] A new completion does not change numbers while the menu is open. Entries beyond nine remain accessible through normal navigation.
- [ ] The shortcut works with the intended keyboard layout. An existing shortcut conflict produces an error without disabling mouse navigation.
- [ ] Failed pane navigation leaves the completion pending. Project groups, elapsed times, and pane titles remain correct.
- [ ] Pane navigation activates Kitty and selects the correct pane.
- [ ] Reload does not duplicate sessions, and closed or stale sessions disappear.
- [ ] Restarting the app restores reporting through heartbeats.
- [ ] The default build has no popovers, and the optional-feature build behaves as documented when tested.

Every pull request that triggers CI must bump `version` in `package.json` and move the `Unreleased` changelog entries under a matching `## [<version>] - <date>` heading; CI fails otherwise. Pull requests that change only Markdown skipped by CI are exempt. After the merge to `main` passes CI, the Release workflow tags that commit `v<version>`. Merges that do not change the version are not tagged.

Update Cargo metadata, the Cargo lockfile, and both app bundle version fields to match the product version. `npm test` and bundling reject version drift. The initial adoption PR validates the new version without requiring a previous package manifest. The source-only Release workflow creates no downloadable app or GitHub release. See [CONTRIBUTING.md](CONTRIBUTING.md) for contribution guidelines.

## License

[MIT](LICENSE) © Philip Hadviger
