# Pi Watch

A first-version native macOS menu bar app for seeing which Pi sessions are working across Kitty panes. Rust + AppKit; no webview, browser server, transcript scanning, or model calls.

## Run

Requires macOS, Rust, and a Pi version exposing `agent_settled` (developed against Pi 1.4.0).

```sh
sh scripts/bundle.sh
open "dist/Pi Watch.app"
```

The menu bar displays `Pi working/total`. Open it to see sessions grouped by working directory, status duration, Kitty pane ID, and process ID. Use the menu's Quit item to exit. For development, `cargo run` also starts the native UI.

## Connect sessions

Launch Pi in each pane with the reporting extension, using an absolute path when outside this checkout:

```sh
pi --extension /absolute/path/to/pi-watch/extensions/pi-watch.ts
```

No global Pi settings are changed. Existing sessions do not become visible automatically; they need to load the extension. You may configure it yourself as a personal extension if you want every project to report. Sessions started without Kitty still appear, with an unknown pane ID.

Working begins at `before_agent_start`/`agent_start`; idle begins only at `agent_settled`, not at the end of an individual run. Idle means automatic processing has stopped, not that an approval dialog was detected. A heartbeat every five seconds lets the app recover after restarting. Sessions become disconnected after 15 seconds without a report and disappear after five minutes. Durations start when the app observes a status change; disconnected duration is time since last contact. Session identity resets on extension reload or session replacement, so a recently disconnected row can temporarily coexist with its replacement.

## Current limitations

- **No click-to-focus yet.** The local Kitty remote-control probe could not open a controlling TTY, and no `KITTY_LISTEN_ON` endpoint was available. Remote-control support is unverified; this version does not invoke it or change Kitty configuration.
- No needs-attention inference, notifications, custom icon, launch-at-login, or persistent history.
- Menu content refreshes once per second while the normal app run loop is running; close and reopen the menu if it stays open across a status change.
- This is a local development build, ad-hoc signed rather than Developer ID signed/notarized.
- The native interface needs a manual visual check in a logged-in macOS desktop session.

## Privacy and IPC

The extension sends only protocol version, random reporting-instance ID, working directory, PID, Kitty window ID, and working/idle/disconnected status. No prompts, transcripts, tool data, or model responses are read or sent. Reports remain in memory.

IPC uses `~/Library/Application Support/Pi Watch/events.sock` in a user-owned directory with mode `0700`; the socket has mode `0600`. Frames are limited to 8 KiB, clients have a short read timeout, and the registry is capped at 512 entries. A process lock prevents two app instances replacing each other's sockets. The socket may remain after exit and is replaced on next startup. Any process running as the same user can submit status: this is not a security boundary against malicious local software under your account.

## Checks

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
node extensions/pi-watch.test.mjs
```

The extension harness requires a Node version with native TypeScript type stripping (Node 22.18+). It checks lifecycle events, unavailable-app handling, IPC payload fields, and settlement semantics without starting Pi or making model requests.
