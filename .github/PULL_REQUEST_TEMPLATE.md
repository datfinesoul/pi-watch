## Summary

Describe the problem and solution.

## Testing

- [ ] Tests pass (`npm test` or `pnpm test`)
- [ ] Package contents validate (`npm run pack:check` or `pnpm run pack:check`)
- [ ] Manually tested in Pi, or the change has no user-visible behavior
- [ ] Native checks pass on macOS (`npm run check:app`), or native validation is unavailable and the reason is recorded.
- [ ] Applicable desktop checks are recorded, or the change needs no desktop check.

## Checklist

- [ ] Tests cover new or changed behavior, or none changed.
- [ ] User-facing documentation is updated, or none is needed.
- [ ] `CHANGELOG.md` is updated for user-visible changes, or there are none.
- [ ] `version` in `package.json` is bumped with a matching dated `CHANGELOG.md` section, or the change touches only Markdown that CI skips.
- [ ] Product versions agree across the extension, Cargo metadata, lockfile, and app bundle.
- [ ] IPC compatibility and upgrade instructions are updated, or the protocol is unchanged.
- [ ] No secrets or private data are included.
- [ ] The change complies with the Code of Conduct and MIT License.
