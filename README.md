# pi-watch

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

A [Pi](https://github.com/earendil-works/pi) extension that shows which other sessions need input or finished a run. It displays a small in-terminal widget and lets you switch between Kitty panes. No companion app, transcript scanning, or model calls are required.

## Installation

### Global installation

Install the extension for every project. Pi records the package in `~/.pi/agent/settings.json`:

```sh
pi install git:github.com/datfinesoul/pi-watch
```

### Project installation

Install the extension for the current project. Pi records the package in `.pi/settings.json`:

```sh
pi install --local git:github.com/datfinesoul/pi-watch
```

Project packages load only after Pi grants project trust. Run `/reload` in existing Pi sessions or start new sessions after installation. Use Pi 1.5.2 or newer and Node.js 22.19 or newer. Pi Watch uses the native `agent_settled`, `ui_prompt_start`, and `ui_prompt_end` events.

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

From the checkout, try the extension without changing Pi settings:

```sh
pi --no-extensions --extension ./extensions/watch/index.ts
```

Use an absolute extension path outside the checkout. The control plane already loads this working tree and disables the global Git package. Do not re-enable the global copy here.

> The former entry point was `extensions/pi-watch.ts`. Update explicit `--extension` paths and remove manually installed old copies to prevent duplicate instances. The native macOS app was removed in 0.6.0. Quit any running copy and remove `~/Applications/Pi Watch.app` or your checkout's `dist/Pi Watch.app` if present. You can also remove `~/Library/Application Support/Pi Watch/` after quitting the old app. Pi Watch does not remove these files or stop the app automatically. Reload all participating Pi sessions to enable input-waiting detection. Older extensions still share completion markers but cannot show input requests.

</details>

## Usage

Load Pi Watch in each Pi session you want to monitor. Use macOS or Linux with Kitty. Each session needs `KITTY_WINDOW_ID` and a Unix `KITTY_LISTEN_ON` endpoint for navigation.

A small line appears above the editor when other sessions need attention:

```text
◇ 1 input · 2 done  [ctrl+' switch]
```

The widget disappears when no other session needs attention. It does not take focus, open dialogs, or issue desktop notifications.

- **Needs input:** A blocking extension dialog remains open, including a question from `pi-questions`. Visiting that pane does not answer the question. The marker remains until the dialog closes.
- **Done:** An agent run settled and nobody inspected it yet. This does not imply task success. A new run or a sampled focus change from unfocused to focused clears the marker.

A pane already focused when its run ends remains pending until you leave and return. Visits between polls can be missed. Other sessions refresh their widgets on the next heartbeat, normally within five seconds.

Configure Kitty remote control outside Pi. Use a private Unix socket in a user-owned directory with mode `0700`. Pi Watch uses the inherited endpoint and never changes Kitty configuration. All participating sessions should run within the same Kitty instance. Navigation uses the current session's socket, so a pane in a different Kitty instance is not a supported target.

To remove the extension, use the installed source, with `--local` for a project installation:

```sh
pi remove git:github.com/datfinesoul/pi-watch
# Or, from the project where it was installed:
pi remove --local git:github.com/datfinesoul/pi-watch
```

For local installations, substitute `/path/to/pi-watch`. Run `/reload` in affected sessions. Project removal leaves any global installation intact.

## Keyboard shortcut

Press **Ctrl+'** inside a participating Pi session to focus the next pane that needs attention. Input requests precede unseen completions. Repeated presses cycle through all listed panes.

The extension accepts legacy BEL and Kitty-protocol CSI forms. It consumes the shortcut only when navigation is available and another session needs attention. Ordinary keys pass through. The shortcut is local to Pi, not a global operating-system shortcut. Navigation failures leave markers unchanged.

## How it works

### Lifecycle and markers

`before_agent_start` and `agent_start` mark the agent as working and clear its previous completion. Only `agent_settled` marks a worked run as complete. `agent_end` is not final because retries and queued work can follow it. Startup and repeated idle settlement events do not create completions.

`ui_prompt_start` marks the session as waiting for input. `ui_prompt_end` clears only that flag. Agent activity and completion remain independent. Pi coalesces nested or overlapping dialogs into one waiting span. Answer, cancellation, abort, and dialog failure close that span through Pi's UI lifecycle.

Detection covers blocking extension selectors, confirmations, inputs, editors, and custom UI. It is not specific to one tool. A custom UI that does not require input can still appear as waiting. Pi Watch does not infer questions from assistant text or tool names.

### Privacy and runtime state

Each session writes one private JSON file under `$XDG_STATE_HOME/pi-watch/sessions/`, or `~/.local/state/pi-watch/sessions/` by default. These are runtime files, not user configuration. Pi Watch reads no configuration files.

State contains only protocol version `1`, process ID, completion flag `pending`, input flag `waiting`, and Kitty pane ID `kitty_window_id`. The optional `waiting` field extends the existing completion format. Older files without it remain valid. Files use mode `0600` in a directory created with mode `0700`. Atomic replacement prevents partially written records.

A five-second heartbeat reads other sessions and samples the current pane's focus with a bounded `kitty ls` request. The extension retains only focus flags. It uses `kitty` from `PATH`, without a shell. Missing commands, invalid responses, and state-file failures do not stop Pi.

A session removes its file and stops its timer and input handler on shutdown. Readers remove records whose processes are no longer alive. PID reuse can keep a stale record until that process exits. Reload and session replacement create a new runtime identity and remove the previous file.

Pi Watch does not read responses, prompts, transcripts, thinking, questions, answers, tool arguments, tool output, or credentials. It stores no pane titles, project paths, or socket endpoints. It has no app IPC socket, network listener, background service, or native build.

Processes under the same user account can submit or change state files. Kitty remote control permits more than pane focusing. Pi Watch is not a sandbox or authorization boundary. See [SECURITY.md](SECURITY.md).

## Development

The extension requires no runtime dependencies beyond the Pi host and has no build step. Runtime helpers use `.ts` files so `/reload` refreshes imported logic. Node.js imports these helpers directly in unit tests.

Run tests and inspect the package with npm or pnpm:

```sh
npm test
npm run pack:check
```

Use `pnpm test` and `pnpm run pack:check` with pnpm. Tests cover lifecycle markers, input waiting, focus acknowledgment, navigation, privacy, cleanup, package contents, and the CI version gate. Set `PI_CODING_AGENT_ENTRY` to an installed Pi `dist/index.js` for installed-Pi loader and UI-event integration tests. These tests make no model requests.

### Manual Pi checks

Use two Pi sessions in the same Kitty instance:

- [ ] A completed run appears as done in the other session.
- [ ] An open `pi-questions` dialog appears as needs input in the other session.
- [ ] Ctrl+' cycles through waiting and completed panes without submitting an answer.
- [ ] Answering or cancelling clears input waiting. A resumed run becomes done only after final settlement.
- [ ] Visiting a completed pane clears completion. Visiting a question leaves input waiting active.
- [ ] Reload and exit remove old markers and input handlers.

Each feature branch that triggers CI gets one version update relative to its base. Keep that version unchanged throughout the branch. Record subsequent user-visible changes under the same matching `## [<version>] - <date>` changelog heading. Do not increment the patch version for each edit, fix, or conversation turn. CI requires the branch version to exceed its base version. Pull requests that change only Markdown skipped by CI are exempt. After merge and successful CI, the Release workflow tags the tested commit `v<version>`. Merges that do not change the version are not tagged.

See [CONTRIBUTING.md](CONTRIBUTING.md) for contribution guidelines.

## License

[MIT](LICENSE) © Philip Hadviger
