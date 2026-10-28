import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { readFileSync, mkdtempSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { root } from "../scripts/check-versions.mjs";

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

test("the adoption gate permits a first manifest with a valid version and dated changelog section", t => {
  const result = runGate(t, {});
  assert.equal(result.status, 0, result.stdout + result.stderr);
  assert.match(result.stdout, /initial package adoption/);
});

test("later CI runs require a real version increase", t => {
  assert.equal(runGate(t, { version: "0.1.0" }).status, 0);
  for (const version of ["0.2.0", "0.3.0"]) {
    const result = runGate(t, { version });
    assert.equal(result.status, 1);
    assert.match(result.stdout, /must bump package.json version/);
  }
});

test("adoption still rejects an invalid version or missing changelog heading", t => {
  assert.equal(runGate(t, {}, "invalid").status, 1);
  assert.equal(runGate(t, {}, "0.2.0", "## [Unreleased]\n").status, 1);
});
