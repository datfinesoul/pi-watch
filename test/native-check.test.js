import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import test from "node:test";
import { root } from "../scripts/check-versions.mjs";

test("native validation fails clearly for missing tools or an unsupported host", () => {
  const result = spawnSync(process.execPath, ["scripts/check-app.mjs"], {
    cwd: root, env: { ...process.env, PATH: "" }, encoding: "utf8",
  });
  assert.equal(result.status, 1);
  if (process.platform === "darwin") {
    assert.match(result.stderr, /Native validation failed:.*ENOENT/);
    assert.match(result.stdout, /cargo fmt/);
  } else {
    assert.match(result.stderr, /requires macOS.*Native validation was not run/);
  }
});
