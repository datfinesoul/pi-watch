import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync, rmSync, cpSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { checkVersions, productVersions, root } from "../scripts/check-versions.mjs";

test("extension, Cargo manifest, lockfile, and app bundle share one product version", () => {
  const versions = productVersions();
  assert.equal(checkVersions(), versions.package);
  assert.equal(Object.keys(versions).length, 5);
});

test("version validation detects drift in every native version field", t => {
  const directory = mkdtempSync(join(tmpdir(), "pi-watch-versions-"));
  t.after(() => rmSync(directory, { recursive: true, force: true }));
  mkdirSync(join(directory, "apps/macos/resources"), { recursive: true });
  for (const file of ["package.json", "apps/macos/Cargo.toml", "apps/macos/Cargo.lock", "apps/macos/resources/Info.plist"]) {
    cpSync(join(root, file), join(directory, file));
  }
  const version = checkVersions(directory);
  for (const [file, text] of [
    ["apps/macos/Cargo.toml", `version = "${version}"`],
    ["apps/macos/Cargo.lock", `name = "pi-watch"\nversion = "${version}"`],
    ["apps/macos/resources/Info.plist", `<key>CFBundleShortVersionString</key><string>${version}</string>`],
    ["apps/macos/resources/Info.plist", `<key>CFBundleVersion</key><string>${version}</string>`],
  ]) {
    const path = join(directory, file);
    const original = readFileSync(path, "utf8");
    assert.ok(original.includes(text));
    writeFileSync(path, original.replace(text, text.replace(version, "99.0.0")));
    assert.throws(() => checkVersions(directory), /differs from package.json/);
    writeFileSync(path, original);
  }
});
