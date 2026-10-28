import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { cpSync, mkdtempSync, mkdirSync, rmSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { tmpdir } from "node:os";
import { pathToFileURL } from "node:url";
import test from "node:test";
import { root } from "../scripts/check-versions.mjs";

function packageFiles() {
  const result = spawnSync("npm", ["pack", "--dry-run", "--json", "--ignore-scripts"], { cwd: root, encoding: "utf8" });
  assert.equal(result.status, 0, result.stderr);
  return JSON.parse(result.stdout)[0].files.map(file => file.path);
}

test("the extension package contains its helpers but no native code, output, or dev harnesses", () => {
  const files = packageFiles();
  for (const required of ["package.json", "extensions/watch/index.ts", "src/reporting.js", "src/metadata.js", "LICENSE", "README.md", "CHANGELOG.md"]) {
    assert.ok(files.includes(required), `missing ${required}`);
  }
  assert.ok(files.every(file => !/^(apps|target|dist|scripts|test)\//.test(file)), files.join("\n"));
});

const entry = process.env.PI_CODING_AGENT_ENTRY;
test("installed Pi loads the packaged entry point without opening sockets or starting the app", { skip: !entry }, async t => {
  const directory = mkdtempSync(join(tmpdir(), "pi-watch-package-"));
  t.after(() => rmSync(directory, { recursive: true, force: true }));
  for (const file of packageFiles()) {
    const destination = join(directory, file);
    mkdirSync(dirname(destination), { recursive: true });
    cpSync(join(root, file), destination);
  }
  const { loadExtensions, createExtensionRuntime } = await import(new URL("./core/extensions/loader.js", pathToFileURL(resolve(entry))));
  const loaded = await loadExtensions([join(directory, "extensions/watch/index.ts")], directory, undefined, createExtensionRuntime());
  assert.deepEqual(loaded.errors, []);
  assert.equal(loaded.extensions.length, 1);
  const extension = loaded.extensions[0];
  assert.deepEqual([...extension.handlers.keys()].sort(), [
    "session_start", "before_agent_start", "agent_start", "agent_settled", "session_shutdown",
  ].sort());
  assert.equal(extension.commands.size, 0);
  assert.equal(extension.tools.size, 0);
  for (const handler of extension.handlers.get("session_shutdown")) await handler({ type: "session_shutdown" }, {});
});
