import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { readFileSync, mkdtempSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
const root = join(import.meta.dirname, "..");

// Exercise the actual CI gate without creating Git history or invoking GitHub Actions.
const workflow = readFileSync(join(root, ".github/workflows/ci.yml"), "utf8");
const gate = workflow.match(/node --input-type=module - <<'SCRIPT'\n([\s\S]*?)\n\s*SCRIPT/)[1];

function runGate(t, base, head = "0.2.0", changelog = "## [0.2.0] - 2026-10-28\n") {
  const directory = mkdtempSync(join(tmpdir(), "pi-watch-gate-"));
  t.after(() => rmSync(directory, { recursive: true, force: true }));
  writeFileSync(join(directory, "base-package.json"), JSON.stringify(base));
  writeFileSync(join(directory, "package.json"), JSON.stringify({ version: head }));
  writeFileSync(join(directory, "CHANGELOG.md"), changelog);
  return spawnSync(process.execPath, ["--input-type=module", "--eval", gate], {
    cwd: directory, env: { ...process.env, RUNNER_TEMP: directory }, encoding: "utf8",
  });
}

test("the baseline gate requires an existing valid base version", t => {
  const result = runGate(t, {});
  assert.equal(result.status, 1);
  assert.match(result.stdout, /Base version.*not valid SemVer/);
});

test("later CI runs require a real version increase", t => {
  assert.equal(runGate(t, { version: "0.1.0" }).status, 0);
  for (const version of ["0.2.0", "0.3.0"]) {
    const result = runGate(t, { version });
    assert.equal(result.status, 1);
    assert.match(result.stdout, /must bump package.json version/);
  }
});

test("the gate rejects an invalid version or missing changelog heading", t => {
  assert.equal(runGate(t, { version: "0.1.0" }, "invalid").status, 1);
  assert.equal(runGate(t, { version: "0.1.0" }, "0.2.0", "## [Unreleased]\n").status, 1);
});
