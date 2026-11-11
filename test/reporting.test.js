import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { setTimeout as pause } from "node:timers/promises";
import test from "node:test";
import { registerReporting, stateDirectory } from "../src/reporting.ts";

async function until(predicate) {
  const deadline = Date.now() + 2000;
  while (!predicate()) {
    assert.ok(Date.now() < deadline, "timed out waiting for state");
    await pause(5);
  }
}
function fixture(t, options = {}) {
  const home = mkdtempSync(join(tmpdir(), "pi-watch-"));
  const handlers = new Map();
  const widgets = new Map();
  const keys = new Set();
  const calls = [];
  const env = { KITTY_WINDOW_ID: "123", KITTY_LISTEN_ON: "unix:/tmp/kitty-test.sock", XDG_STATE_HOME: join(home, "state"), ...options.env };
  const dir = stateDirectory(home, env, options.platform ?? "darwin");
  const ui = {
    setWidget: (key, value) => value === undefined ? widgets.delete(key) : widgets.set(key, value),
    onTerminalInput: handler => { keys.add(handler); return () => keys.delete(handler); },
  };
  const ctx = { cwd: "/tmp/project", hasUI: true, mode: "tui", ui, isIdle: () => true };
  registerReporting({ on: (name, handler) => handlers.set(name, handler) }, {
    home, env, pid: 42, platform: "darwin", alive: () => true,
    runCommand: async (file, args) => { calls.push([file, args]); return ""; },
    ...options,
    // Merge environment overrides without discarding the isolated state directory.
    env,
  });
  const emit = (event, payload = {}, context = ctx) => handlers.get(event)(payload, context);
  t.after(() => { emit("session_shutdown"); rmSync(home, { recursive: true, force: true }); });
  const files = () => { try { return readdirSync(dir).filter(file => file.endsWith(".json")); } catch { return []; } };
  const ownFile = () => files().find(file => JSON.parse(readFileSync(join(dir, file), "utf8")).pid === 42);
  return {
    home, dir, ctx, emit, handlers, widgets, keys, calls, files,
    start: (idle = true) => emit("session_start", {}, { ...ctx, isIdle: () => idle }),
    own: () => JSON.parse(readFileSync(join(dir, ownFile()), "utf8")),
    ownPath: () => join(dir, ownFile()),
    other: ({ pid = 43, pending = true, waiting, pane = "777", version = 1 } = {}) => {
      mkdirSync(dir, { recursive: true });
      const path = join(dir, `other-${pane}-${pid}.json`);
      writeFileSync(path, JSON.stringify({ version, pid, pending, waiting, kitty_window_id: pane }));
      return path;
    },
    press: data => [...keys][0]?.(data),
  };
}

const focusOutput = focused => JSON.stringify([{
  is_focused: true, tabs: [{ is_focused: true, windows: [{ id: 123, is_focused: focused, title: "private" }] }],
}]);

test("runtime state keeps the existing XDG location on macOS and Linux", () => {
  assert.equal(stateDirectory("/home/user", {}, "linux"), "/home/user/.local/state/pi-watch/sessions");
  assert.equal(stateDirectory("/home/user", { XDG_STATE_HOME: "/state" }, "darwin"), "/state/pi-watch/sessions");
  assert.equal(stateDirectory("/home/user", { XDG_STATE_HOME: "relative" }, "linux"), "/home/user/.local/state/pi-watch/sessions");
  assert.equal(stateDirectory("/home/user", {}, "win32"), null);
});

test("only final settlement marks completion and startup does not", async t => {
  const f = fixture(t);
  await f.start();
  assert.deepEqual(f.own(), { version: 1, pid: 42, pending: false, waiting: false, kitty_window_id: "123" });
  assert.equal(f.handlers.has("agent_end"), false);
  assert.equal(f.handlers.has("message_end"), false, "no response inspection");
  f.emit("agent_settled");
  assert.equal(f.own().pending, false);
  f.emit("before_agent_start");
  f.emit("agent_start");
  f.emit("agent_settled");
  assert.equal(f.own().pending, true);
  f.emit("agent_settled");
  assert.equal(f.own().pending, true, "idle re-settlement preserves completion");
  f.emit("agent_start");
  assert.equal(f.own().pending, false);
  f.emit("agent_settled");
  f.emit("before_agent_start");
  assert.equal(f.own().pending, false);
});

test("a busy startup can settle and replacement resets state without orphan files", async t => {
  const f = fixture(t);
  await f.start(false);
  const first = f.ownPath();
  f.emit("ui_prompt_start");
  f.emit("agent_settled");
  assert.equal(f.own().pending, true);
  await f.start();
  assert.notEqual(f.ownPath(), first);
  assert.equal(f.files().length, 1);
  assert.equal(f.own().waiting, false);
  assert.equal(f.own().pending, false);
  assert.equal(f.keys.size, 1);
});

for (const outcome of ["answered", "cancelled", "aborted", "failed"]) {
  test(`prompt ${outcome}: waiting ends without false completion and work resumes`, async t => {
    const f = fixture(t);
    await f.start();
    f.emit("agent_start");
    f.emit("ui_prompt_start", { kind: "custom", title: "must not be stored" });
    assert.equal(f.own().waiting, true);
    assert.equal(f.own().pending, false);
    f.emit("ui_prompt_end", { outcome });
    assert.equal(f.own().waiting, false);
    assert.equal(f.own().pending, false);
    f.emit("agent_settled");
    assert.equal(f.own().pending, true);
  });
}

test("an idle dialog creates no completion and preserves a previous completion", async t => {
  const f = fixture(t);
  await f.start();
  f.emit("ui_prompt_start");
  f.emit("ui_prompt_end");
  f.emit("agent_settled");
  assert.equal(f.own().pending, false);
  f.emit("agent_start");
  f.emit("agent_settled");
  f.emit("ui_prompt_start");
  f.emit("ui_prompt_end");
  assert.equal(f.own().pending, true);
});

test("settlement while a prompt remains open preserves input waiting", async t => {
  const f = fixture(t);
  await f.start();
  f.emit("agent_start");
  f.emit("ui_prompt_start");
  f.emit("agent_settled");
  assert.equal(f.own().waiting, true);
  f.emit("ui_prompt_end");
  assert.equal(f.own().waiting, false);
  assert.equal(f.own().pending, true);
});

test("focus acknowledges completion but never an unanswered prompt", async t => {
  let focused = true;
  const f = fixture(t, { heartbeatMs: 10, runCommand: async () => focusOutput(focused) });
  await f.start();
  f.emit("agent_start");
  f.emit("agent_settled");
  await pause(40);
  assert.equal(f.own().pending, true, "already focused is not a new visit");
  f.emit("ui_prompt_start");
  focused = false;
  await pause(40);
  focused = true;
  await until(() => !f.own().pending);
  assert.equal(f.own().waiting, true, "visiting does not answer a prompt");
  f.emit("ui_prompt_end");
  assert.equal(f.own().waiting, false);
});

test("unknown focus does not acknowledge completion", async t => {
  const f = fixture(t, { heartbeatMs: 10 });
  await f.start();
  f.emit("agent_start");
  f.emit("agent_settled");
  await pause(40);
  assert.equal(f.own().pending, true);
});

test("widget distinguishes input and completion, reads older files, and cycles input first", async t => {
  const f = fixture(t);
  await f.start();
  f.other(); // Older extension file: no waiting field.
  const waitingPath = f.other({ pid: 44, pane: "888", pending: false, waiting: true });
  f.emit("before_agent_start");
  assert.deepEqual(f.widgets.get("pi-watch"), ["◇ 1 needs input elsewhere · 1 done elsewhere — ctrl+' switch"]);
  for (const [key, pane] of [["\x1b[39;5u", "888"], ["\x07", "777"], ["\x1b[39;6u", "888"], ["\x1b[27;5;39~", "777"]]) {
    assert.deepEqual(f.press(key), { consume: true });
    assert.deepEqual(f.calls.at(-1), ["kitty", ["@", "--to", "unix:/tmp/kitty-test.sock", "focus-window", "--match", `id:${pane}`]]);
  }
  assert.equal(f.press("x"), undefined);
  assert.equal(JSON.parse(readFileSync(waitingPath)).waiting, true, "navigation never changes another session's file");
  rmSync(waitingPath);
  f.emit("before_agent_start");
  assert.deepEqual(f.widgets.get("pi-watch"), ["◇ 1 done elsewhere — ctrl+' switch"]);
});

test("multiple input requests use a separate count without double counting completions", async t => {
  const f = fixture(t);
  await f.start();
  f.other({ waiting: true });
  f.other({ pane: "888", pid: 44, pending: false, waiting: true });
  f.emit("ui_prompt_start");
  assert.deepEqual(f.widgets.get("pi-watch"), ["◇ 2 need input elsewhere — ctrl+' switch"]);
});

test("heartbeat discovers requests and removes resolved requests without another agent event", async t => {
  const f = fixture(t, { heartbeatMs: 10 });
  await f.start();
  const path = f.other({ pending: false, waiting: true });
  await until(() => f.widgets.has("pi-watch"));
  rmSync(path);
  await until(() => !f.widgets.has("pi-watch"));
  assert.equal(f.press("\x07"), undefined);
});

test("dead, malformed, own-process, same-pane, and unsupported records do not count", async t => {
  const f = fixture(t, { alive: pid => pid !== 99 });
  await f.start();
  const dead = f.other({ pid: 99 });
  f.other({ pid: 42, pane: "555" });
  f.other({ pane: "123" });
  f.other({ pane: "not-a-pane" });
  f.other({ version: 99, pane: "666" });
  f.other({ pid: -1 });
  f.other({ pid: 1.5 });
  writeFileSync(join(f.dir, "broken.json"), "{");
  writeFileSync(join(f.dir, "ignored.json.tmp"), "{");
  f.emit("before_agent_start");
  assert.equal(f.widgets.has("pi-watch"), false);
  assert.equal(f.files().includes(dead.split("/").at(-1)), false);
});

test("Linux publishes the same minimal private state with no Git or app operations", async t => {
  const f = fixture(t, { platform: "linux" });
  await f.start();
  f.emit("ui_prompt_start", { title: "secret", question: "secret" });
  assert.equal(statSync(f.dir).mode & 0o777, 0o700);
  assert.equal(statSync(f.ownPath()).mode & 0o777, 0o600);
  assert.deepEqual(Object.keys(f.own()).sort(), ["version", "pid", "pending", "waiting", "kitty_window_id"].sort());
  assert.ok(f.calls.every(([file, args]) => file === "kitty" && args.includes("ls")));
  assert.ok(readdirSync(f.dir).every(file => file.endsWith(".json")), "no temporary files remain");
});

test("without Kitty navigation the shortcut passes through and no commands run", async t => {
  const f = fixture(t, { env: { KITTY_WINDOW_ID: undefined, KITTY_LISTEN_ON: undefined } });
  await f.start();
  f.other();
  f.emit("agent_start");
  f.emit("agent_settled");
  assert.equal(f.calls.length, 0);
  assert.equal(f.widgets.size, 0);
  assert.equal(f.press("\x07"), undefined);
  assert.equal(f.own().pending, true);
});

for (const mode of ["print", "json", "rpc"]) {
  test(`${mode} mode does not install terminal widgets or key handlers`, async t => {
    const f = fixture(t);
    await f.emit("session_start", {}, { ...f.ctx, mode, hasUI: mode === "rpc" });
    f.other();
    f.emit("ui_prompt_start", {}, { ...f.ctx, mode, hasUI: mode === "rpc" });
    assert.equal(f.widgets.size, 0);
    assert.equal(f.keys.size, 0);
    assert.equal(f.own().waiting, true);
  });
}

test("shutdown removes state, widget, keys, and heartbeat and ignores late prompt events", async t => {
  const f = fixture(t, { heartbeatMs: 10 });
  await f.start();
  f.other();
  await until(() => f.widgets.has("pi-watch"));
  f.emit("session_shutdown");
  const calls = f.calls.length;
  f.emit("ui_prompt_end");
  f.emit("ui_prompt_start");
  f.emit("agent_settled");
  f.emit("session_shutdown");
  await pause(40);
  assert.equal(f.files().length, 1, "only the other session's file remains");
  assert.equal(f.keys.size, 0);
  assert.equal(f.widgets.size, 0);
  assert.equal(f.calls.length, calls);
});

test("shutdown during a focus request cannot restart the heartbeat or recreate state", async t => {
  let release;
  const f = fixture(t, { runCommand: () => new Promise(resolve => { release = resolve; }), heartbeatMs: 10 });
  const starting = f.start();
  f.emit("session_shutdown");
  release(focusOutput(true));
  await starting;
  await pause(40);
  assert.equal(f.files().length, 0);
  assert.equal(f.keys.size, 0);
});
