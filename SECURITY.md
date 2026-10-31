# Security Policy

## Supported versions

Security updates are provided for the latest released version on a best-effort basis.

## Reporting a vulnerability

Please do not report vulnerabilities in a public issue. Use GitHub's private vulnerability reporting feature on the repository's **Security** tab. If private reporting is unavailable, contact the maintainer through the contact method listed on the GitHub profile at <https://github.com/datfinesoul>.

Include a description of the issue, affected versions, reproduction steps, potential impact, and any suggested remediation. You should receive an acknowledgment within seven days. Please allow a reasonable amount of time for investigation and remediation before public disclosure.

## Security model

Pi extensions execute with the same operating-system permissions as Pi. Review extension source before installation and protect the package files from untrusted modification. The extension reports session identity, working directory, status, project labels, process ID, and Kitty metadata to a local companion app. It does not inspect prompts, transcripts, tool arguments, credentials, or model responses. It reads no user configuration files and does not launch the app.

The app accepts bounded JSON reports through `~/Library/Application Support/Pi Watch/events.sock` on macOS, or `~/.local/share/Pi Watch/events.sock` (`$XDG_DATA_HOME/Pi Watch` when set) on Linux. Its directory uses mode `0700`, and the socket uses mode `0600`. The app checks directory ownership, rejects a symlink at that directory, limits frames to 8 KiB, and caps the registry at 512 entries. An app lock prevents concurrent instances from replacing a live socket. State remains in memory. Socket and lock files may remain after exit.

Reports are not authenticated against other processes under the same user account. Such processes can submit status and Kitty endpoints. Kitty remote control permits more than pane focusing. The app invokes fixed navigation commands without a shell, but local same-user software remains trusted. Do not expose Kitty control sockets to other users or configure remote control automatically.

Pane titles and directory names can contain private information. Avoid sensitive pane titles and redact paths and socket endpoints from public bug reports. The macOS source-built app is ad-hoc signed, not Developer ID signed or notarized. The Linux app is a cargo-built binary and is not signed. This product is a convenience, not a sandbox, authorization control, or security boundary.
