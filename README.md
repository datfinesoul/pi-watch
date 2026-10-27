# Pi Watch

A first-version native macOS menu bar app for seeing which Pi sessions are working across Kitty panes. Rust + AppKit; no webview, browser server, transcript scanning, or model calls.

## Run

Requires macOS, Rust, Apple command-line build tools, and a Pi version exposing `agent_settled` (developed against Pi 1.4.0). The extension uses `git` and `kitty` from the Pi process's `PATH` for project labels, pane titles, and focus detection. Without these commands, status reporting still works with fallback labels and unknown focus. This module remains experimental and does not follow the control plane's shared extension-module layout.

```sh
sh scripts/bundle.sh
open "dist/Pi Watch.app"
```

To rebuild and restart the app from this directory, run `make restart`. It builds the app, stops the running copy, waits for exit, and opens the new build. Use `make build` to build without restarting, `make stop` to close it, or `make open` to open the existing build. Run `make help` to list the targets.

From the control-plane root, use `make -C pi-watch restart`. The `-C pi-watch` option selects this module's directory before Make runs the target. App-only changes require an app restart. After extension changes, restart the app first if its protocol changed, then run `/reload` in each reporting Pi session.

The menu bar shows `π ● N ○ M` for connected sessions only. The filled circle means active. The empty circle means idle. An additional `! N` counts unseen idle transitions.

Open the menu to see connected sessions grouped by working directory, with status duration and Kitty pane titles. Headings show compact Git labels, such as `↳/superproject/repo/src` or `repo/src` for a standalone repository. Non-Git directories use their directory name. Pane numbers replace unavailable titles. The UI does not show process IDs, although IPC still uses them to deduplicate reloads.

Use the menu's Quit item to exit. For development, `cargo run` also starts the native UI. Stop the bundled app before a development run because only one app instance can own the reporting socket.

## Connect sessions

Launch Pi in each pane with the reporting extension, using an absolute path when outside this checkout:

```sh
pi --extension /absolute/path/to/pi-watch/extensions/pi-watch.ts
```

No global Pi settings are changed. The `--extension` command loads reporting for that Pi process only. Existing sessions must load the extension before they can report. You may configure it yourself as a personal extension if you want every project to report. Sessions started without Kitty still appear, with an unknown pane ID.

For pane navigation and focus detection, configure Kitty remote control outside Pi. Start Pi in a Kitty pane with `KITTY_WINDOW_ID` and a Unix `KITTY_LISTEN_ON` endpoint. Prefer socket-only access inside a user-owned directory with mode `0700`. Pi Watch does not configure Kitty or discover another endpoint when those values are missing. Git labels, pane titles, and focus state refresh with the five-second heartbeat.

At session startup, reporting uses Pi's current idle state. Active processing starts at `before_agent_start`/`agent_start` and returns to idle at `agent_settled`, not at the end of an individual run. Idle means automatic processing stopped, not that an approval dialog appeared.

A heartbeat every five seconds lets the app recover after restarting. The app hides sessions after a disconnect report or 15 seconds without a report. Entries expire after five minutes. The app removes expired entries when another report arrives. Durations start when the app observes a status change. A new reporting instance replaces the previous entry from the same Pi process, so reloads and session replacements do not create duplicate rows.

An unseen active-to-idle transition adds `! N` to the menu bar and `!` to that session's row. The count includes connected sessions only. Successful navigation from Pi Watch clears the marker. Opening the menu alone does not clear it. Manual navigation clears it when Kitty reports focus, normally within five seconds. Unknown focus does not clear a marker.

A pane already in focus can briefly show a marker until the next focus report. A visit between focus polls can be missed. A new active run, disconnect, stale reconnect, or reporting-instance replacement resets the marker. App restarts discard all markers because state remains in memory. Startup and repeated idle reports do not create markers. A marker indicates an unseen idle transition, not task success or an approval request.

Popovers are disabled by default behind the Cargo feature `popovers`. To try them, stop the bundled app with `make stop`, then run `cargo run --features popovers`. They appear on active-to-idle transitions and close after about five seconds or an outside click. Completions observed together share one popover. The standard bundle and `make restart` use the default features, so popovers remain disabled. Other notification options remain deferred: macOS notification banners and recent changes inside the menu.

## Current limitations

- Click a connected session to focus its Kitty pane. The app then activates Kitty on macOS. Navigation requires socket-based Kitty remote control, `KITTY_WINDOW_ID`, and `KITTY_LISTEN_ON` in each reporting Pi process. Rows without a socket endpoint or pane ID remain disabled. The app searches `/Applications/kitty.app` and `~/Applications/kitty.app`. Command failures appear in the app's standard error output. The app does not change Kitty configuration.
- No needs-attention inference, macOS notification banners, custom icon, launch-at-login, or persistent history.
- Menu content refreshes once per second while the normal app run loop is running; close and reopen the menu if it stays open across a status change.
- This is a local development build, ad-hoc signed rather than Developer ID signed/notarized.
- The native interface needs a manual visual check in a logged-in macOS desktop session.

## Privacy and IPC

Reports contain protocol version, random reporting-instance ID, working directory, project label, PID, and working/idle/disconnected status. Kitty metadata includes pane ID, pane title, control socket endpoint, and focus state. Git commands provide project labels. A bounded Kitty listing request selects the reporting pane and retains only its title and focus flags. Other listing fields are discarded, not logged or sent. Titles can contain text set by terminal applications, so avoid sensitive pane titles. The extension does not inspect Pi prompts, transcripts, tool arguments, or model responses. Reports remain in memory.

IPC uses `~/Library/Application Support/Pi Watch/events.sock` in a user-owned directory with mode `0700`; the socket has mode `0600`. Frames are limited to 8 KiB, clients have a short read timeout, and the registry is capped at 512 entries. A process lock prevents two app instances replacing each other's sockets. The socket may remain after exit and is replaced on next startup. Any process running as the same user can submit status: this is not a security boundary against malicious local software under your account.

## Checks

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo test --all-targets
node extensions/pi-watch.test.mjs
```

The extension harness requires a Node version with native TypeScript type stripping (Node 22.18+). It checks lifecycle events, unavailable-app handling, IPC fields, project labels, title parsing, focus flags, and settlement semantics. Rust tests check status transitions, reload deduplication, waiting markers, navigation acknowledgement, and payload validation. These checks do not start Pi or make model requests.

For a desktop check, navigate to another pane while a reporting session processes a prompt. When it becomes idle, check its `!` marker. Return through Kitty or Pi Watch and check that the marker clears. Also check reload deduplication, closed-session removal, pane titles, and macOS activation. Automated checks do not check AppKit placement, desktop focus behavior, or live Kitty permission handling.
