# Security Policy

## Supported versions

Security updates are provided for the latest released version on a best-effort basis.

## Reporting a vulnerability

Please do not report vulnerabilities in a public issue. Use GitHub's private vulnerability reporting feature on the repository's **Security** tab. If private reporting is unavailable, contact the maintainer through the contact method listed on the GitHub profile at <https://github.com/datfinesoul>.

Include a description of the issue, affected versions, reproduction steps, potential impact, and any suggested remediation. You should receive an acknowledgment within seven days. Please allow a reasonable amount of time for investigation and remediation before public disclosure.

## Security model

Pi extensions execute with the same operating-system permissions as Pi. Review extension source before installation and protect the package files from untrusted modification. Pi Watch shares only a protocol version, process ID, completion flag, input-waiting flag, and Kitty pane ID. It does not inspect prompts, transcripts, questions, answers, tool arguments, credentials, or model responses. It reads no user configuration files.

Runtime state lives under `$XDG_STATE_HOME/pi-watch/sessions/`, or `~/.local/state/pi-watch/sessions/` by default. The extension creates the directory with mode `0700` and files with mode `0600`. Keep that directory private. Existing directory permissions remain unchanged. The extension removes its file on shutdown and removes dead-process records when reading other sessions. Abrupt exits can leave files until another reader removes them. PID reuse can delay this cleanup.

State files are not authenticated against other processes under the same user account. Such processes can change status or submit pane IDs. Pi Watch uses only the inherited Kitty socket endpoint and invokes fixed navigation commands without a shell. Kitty remote control permits more than pane focusing. Do not expose Kitty control sockets to other users. Pi Watch never changes Kitty configuration.

Pi Watch has no companion app, app IPC socket, network listener, or response previews. Redact private paths and socket endpoints from bug reports. This product is a convenience, not a sandbox, authorization control, or security boundary.
