# Pi Watch

A first-version native macOS menu bar app for seeing which Pi sessions are working across Kitty panes. Rust + AppKit; no webview, browser server, transcript scanning, or model calls.

## Run

Requires macOS, Rust, and a Pi version exposing `agent_settled` (developed against Pi 1.4.0).

```sh
sh scripts/bundle.sh
open "dist/Pi Watch.app"
```

The menu bar displays `π ● N ○ M` for connected sessions only. The filled circle means active. The empty circle means idle. Open it to see connected sessions grouped by working directory, status duration, Kitty pane ID, and process ID. Use the menu's Quit item to exit. For development, `cargo run` also starts the native UI.

## Connect sessions

Launch Pi in each pane with the reporting extension, using an absolute path when outside this checkout:

```sh
pi --extension /absolute/path/to/pi-watch/extensions/pi-watch.ts
```

No global Pi settings are changed. Existing sessions do not become visible automatically; they need to load the extension. You may configure it yourself as a personal extension if you want every project to report. Sessions started without Kitty still appear, with an unknown pane ID.

Working begins at `before_agent_start`/`agent_start`; idle begins only at `agent_settled`, not at the end of an individual run. Idle means automatic processing has stopped, not that an approval dialog was detected. A heartbeat every five seconds lets the app recover after restarting. The app hides sessions after a disconnect report or 15 seconds without a report. It removes stored entries after five minutes. Durations start when the app observes a status change. A new reporting instance replaces the previous entry from the same Pi process, so extension reloads and session replacements do not create duplicate rows.

## Current limitations

- Click a connected session to focus its Kitty pane and activate Kitty on macOS. This requires socket-based Kitty remote control and `KITTY_LISTEN_ON` in each reporting Pi process. Reload the reporting extension after this update. Rows without a socket endpoint or pane ID remain disabled. The app searches `/Applications/kitty.app` and `~/Applications/kitty.app`. Command failures appear in the app's standard error output. The app does not change Kitty configuration.
- No needs-attention inference, notifications, custom icon, launch-at-login, or persistent history.
- Menu content refreshes once per second while the normal app run loop is running; close and reopen the menu if it stays open across a status change.
- This is a local development build, ad-hoc signed rather than Developer ID signed/notarized.
- The native interface needs a manual visual check in a logged-in macOS desktop session.

## Privacy and IPC

The extension sends only protocol version, random reporting-instance ID, working directory, PID, Kitty window ID, Kitty control socket endpoint, and working/idle/disconnected status. No prompts, transcripts, tool data, or model responses are read or sent. Reports remain in memory.

IPC uses `~/Library/Application Support/Pi Watch/events.sock` in a user-owned directory with mode `0700`; the socket has mode `0600`. Frames are limited to 8 KiB, clients have a short read timeout, and the registry is capped at 512 entries. A process lock prevents two app instances replacing each other's sockets. The socket may remain after exit and is replaced on next startup. Any process running as the same user can submit status: this is not a security boundary against malicious local software under your account.

## Checks

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
node extensions/pi-watch.test.mjs
```

The extension harness requires a Node version with native TypeScript type stripping (Node 22.18+). It checks lifecycle events, unavailable-app handling, IPC payload fields, and settlement semantics without starting Pi or making model requests.
