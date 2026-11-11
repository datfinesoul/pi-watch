import assert from "node:assert/strict";
import { mkdtempSync, readdirSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { setImmediate as tick } from "node:timers/promises";
import test from "node:test";
import { registerReporting, stateDirectory } from "../src/reporting.ts";

const entry = process.env.PI_CODING_AGENT_ENTRY;
// Exercise the installed host's actual UI wrapper and event coalescing, with a
// fake dialog implementation. No model, terminal, Kitty command, or app needed.
test("installed Pi emits balanced custom-UI spans for answers, cancellation, failure, and overlap", { skip: !entry }, async t => {
  const { ExtensionRunner } = await import(pathToFileURL(join(dirname(resolve(entry)), "core/extensions/runner.js")));
  const home = mkdtempSync(join(tmpdir(), "pi-watch-ui-"));
  const handlers = new Map();
  registerReporting({ on: (name, handler) => handlers.set(name, handler) }, { home, env: {}, platform: "linux" });
  t.after(() => { handlers.get("session_shutdown")(); rmSync(home, { recursive: true, force: true }); });
  const ctx = { cwd: home, hasUI: false, mode: "print", isIdle: () => true };
  await handlers.get("session_start")({}, ctx);
  const dir = stateDirectory(home, {}, "linux");
  const state = () => JSON.parse(readFileSync(join(dir, readdirSync(dir).find(file => file.endsWith(".json"))), "utf8"));
  const events = [];
  const runner = Object.create(ExtensionRunner.prototype);
  runner.uiPromptDepth = 0;
  runner.emit = async event => {
    events.push(event.type);
    await handlers.get(event.type)?.(event, ctx);
  };
  for (const result of ["answer", undefined]) {
    let close;
    const ui = runner.wrapUIPromptContext({ custom: () => new Promise(resolve => { close = resolve; }) });
    handlers.get("agent_start")({}, ctx);
    const dialog = ui.custom(() => {});
    await tick();
    assert.equal(state().waiting, true);
    assert.equal(state().pending, false);
    close(result);
    assert.equal(await dialog, result);
    await tick();
    assert.equal(state().waiting, false);
    assert.equal(state().pending, false);
  }
  for (const synchronous of [false, true]) {
    const ui = runner.wrapUIPromptContext({ custom: () => {
      if (synchronous) throw new Error("dialog failed");
      return Promise.reject(new Error("dialog failed"));
    } });
    await assert.rejects(async () => ui.custom(() => {}), /dialog failed/);
    await tick();
    assert.equal(state().waiting, false, "both failure paths end waiting");
  }
  const closes = [];
  const ui = runner.wrapUIPromptContext({ custom: () => new Promise(resolve => closes.push(resolve)) });
  const startCount = events.filter(event => event === "ui_prompt_start").length;
  const first = ui.custom(() => {});
  const second = ui.custom(() => {});
  await tick();
  assert.equal(events.filter(event => event === "ui_prompt_start").length, startCount + 1);
  assert.equal(state().waiting, true);
  closes[0](undefined); // Same custom-UI close path used by pi-questions on abort.
  await first;
  await tick();
  assert.equal(state().waiting, true, "one overlapping dialog remains");
  closes[1]("answer");
  await second;
  await tick();
  assert.equal(state().waiting, false);
  assert.equal(events.filter(event => event === "ui_prompt_start").length, events.filter(event => event === "ui_prompt_end").length);
  handlers.get("agent_settled")({}, ctx);
  assert.equal(state().pending, true);
});
