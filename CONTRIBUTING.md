# Contributing

Thank you for contributing to pi-watch.

## Before you start

- Search existing issues and pull requests before opening a new one.
- Open an issue before making a substantial behavioral or architectural change.
- Report security issues according to [SECURITY.md](SECURITY.md), not in a public issue.
- Follow the [Code of Conduct](CODE_OF_CONDUCT.md) in all project spaces.

## Development setup

Requirements:

- Node.js 22.19 or newer, matching Pi's minimum supported version
- Pi 1.4.0 or newer for extension smoke testing
- macOS, Rust 1.99.0+ with rustfmt and Clippy, and Apple command-line build tools for native changes

Clone the repository and run the tests:

```sh
git clone https://github.com/datfinesoul/pi-watch.git
cd pi-watch
npm test   # or: pnpm test
```

The extension needs no dependency installation or build step. The app has separate locked Cargo dependencies under `apps/macos/`. To load the working tree without enabling other extensions, run:

```sh
pi --no-extensions --extension ./extensions/watch/index.ts
```

On macOS, run `npm run check:app` for native formatting, linting, tests, and bundle creation. It does not start the app. Use `make restart` only when you want to rebuild and restart the checkout app. See the README's [manual desktop checks](README.md#manual-desktop-checks) after UI or navigation changes.

## Making changes

1. Create a focused branch from `main`.
2. Add or update tests for behavior changes.
3. Keep the extension dependency-free unless a dependency provides clear value that cannot reasonably be implemented with the Node.js standard library.
4. Update the README and changelog when user-visible behavior changes.
5. Run `npm test` and `npm run pack:check`, or the `pnpm` equivalents.
6. Run `npm run check:app` on macOS and report any unrun manual desktop checks.
7. Keep the product version consistent in `package.json`, Cargo metadata, the lockfile, and both app bundle version fields. Keep IPC protocol versions independent.

## Pull requests

Pull requests should explain the problem, the chosen solution, testing performed, and any compatibility implications. Keep changes focused and avoid unrelated formatting or refactoring. By submitting a contribution, you agree that it is licensed under the repository's [MIT License](LICENSE).

## Adding contributors

Contributors may add themselves to [CONTRIBUTORS.md](CONTRIBUTORS.md) in the same pull request as their contribution. Use a name or handle and an optional profile link; do not add another person's private contact information.
