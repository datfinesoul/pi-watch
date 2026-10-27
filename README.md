# Pi Watch

A first-version native macOS menu bar app for seeing which Pi sessions are working across Kitty panes. Rust + AppKit; no webview, browser server, transcript scanning, or model calls.

## Run

Requires macOS, Rust, and a Pi version exposing `agent_settled` (developed against Pi 1.4.0).

```sh
sh scripts/bundle.sh
open "dist/Pi Watch.app"
```

To rebuild and restart the app from this directory, run `make restart`. Use `make stop` to close it or `make open` to open the existing build.

The menu bar displays `π ● N ○ M` for connected sessions only. The filled circle means active. The empty circle means idle. Open it to see connected sessions grouped by project, with status duration and Kitty pane titles. Project headings show the superproject and relative repo path, or the standalone repo name. Pane numbers replace unavailable titles. Use the menu's Quit item to exit. For development, `cargo run` also starts the native UI.

## Connect sessions

Launch Pi in each pane with the reporting extension, using an absolute path when outside this checkout:

```sh
pi --extension /absolute/path/to/pi-watch/extensions/pi-watch.ts
```

No global Pi settings are changed. Existing sessions do not become visible automatically; they need to load the extension. You may configure it yourself as a personal extension if you want every project to report. Sessions started without Kitty still appear, with an unknown pane ID.

Working begins at `before_agent_start`/`agent_start`; idle begins only at `agent_settled`, not at the end of an individual run. Idle means automatic processing has stopped, not that an approval dialog was detected. A heartbeat every five seconds lets the app recover after restarting. The app hides sessions after a disconnect report or 15 seconds without a report. It removes stored entries after five minutes. Durations start when the app observes a status change. A new reporting instance replaces the previous entry from the same Pi process, so extension reloads and session replacements do not create duplicate rows.

An unseen active-to-idle transition adds `! N` to the menu bar and `!` to that session's row. The count includes connected sessions only. Successful navigation from Pi Watch clears the marker. Manual navigation clears it when Kitty reports focus, normally within five seconds. Unknown focus does not clear a marker. A new active run, disconnect, stale reconnect, or reporting-instance replacement resets it. Startup and repeated idle reports do not create markers. This indicates an unseen idle transition, not task success or an approval request.

Popovers are disabled by default behind the Cargo feature `popovers`. To try them in a development run, use `cargo run --features popovers`. They appear on active-to-idle transitions and close after about five seconds. Other notification options remain deferred: macOS notification banners and recent changes inside the menu.

## Current limitations

- Click a connected session to focus its Kitty pane and activate Kitty on macOS. This requires socket-based Kitty remote control and `KITTY_LISTEN_ON` in each reporting Pi process. Reload the reporting extension after this update. Rows without a socket endpoint or pane ID remain disabled. The app searches `/Applications/kitty.app` and `~/Applications/kitty.app`. Command failures appear in the app's standard error output. The app does not change Kitty configuration.
- No needs-attention inference, macOS notification banners, custom icon, launch-at-login, or persistent history.
- Menu content refreshes once per second while the normal app run loop is running; close and reopen the menu if it stays open across a status change.
- This is a local development build, ad-hoc signed rather than Developer ID signed/notarized.
- The native interface needs a manual visual check in a logged-in macOS desktop session.

## Privacy and IPC

The extension sends protocol version, random reporting-instance ID, working directory, project label, PID, Kitty window ID, Kitty pane title, Kitty control socket endpoint, Kitty focus state, and working/idle/disconnected status. Git commands provide project labels. A bounded Kitty listing request selects the reporting pane and retains only its title and focus flags. Other listing fields are discarded, not logged or sent. Titles can contain text set by terminal applications, so avoid sensitive pane titles. The extension does not inspect Pi prompts, transcripts, tool arguments, or model responses. Reports remain in memory.

IPC uses `~/Library/Application Support/Pi Watch/events.sock` in a user-owned directory with mode `0700`; the socket has mode `0600`. Frames are limited to 8 KiB, clients have a short read timeout, and the registry is capped at 512 entries. A process lock prevents two app instances replacing each other's sockets. The socket may remain after exit and is replaced on next startup. Any process running as the same user can submit status: this is not a security boundary against malicious local software under your account.

## Checks

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
node extensions/pi-watch.test.mjs
```

The extension harness requires a Node version with native TypeScript type stripping (Node 22.18+). It checks lifecycle events, unavailable-app handling, IPC payload fields, and settlement semantics without starting Pi or making model requests.
