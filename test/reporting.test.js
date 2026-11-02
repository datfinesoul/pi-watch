import assert from "node:assert/strict";
import { createServer } from "node:net";
import { mkdtemp, mkdir, rm } from "node:fs/promises";
import { existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { randomUUID } from "node:crypto";
import { spawn } from "node:child_process";
import { once } from "node:events";
import { setTimeout as pause } from "node:timers/promises";
import test from "node:test";
import { ipcDirectory, registerReporting, responsePreview, stateDirectory } from "../src/reporting.js";

async function until(predicate) {
  const deadline = Date.now() + 2000;
  while (!predicate()) {
    assert.ok(Date.now() < deadline, "timed out waiting for a report");
    await pause(5);
  }
}

async function fixture(t, options = {}) {
  const home = await mkdtemp("/tmp/pw-");
  const handlers = new Map();
  const widgets = new Map();
  const keyHandlers = [];
  const ui = {
    setWidget: (key, content) => { if (content === undefined) widgets.delete(key); else widgets.set(key, content); },
    onTerminalInput: handler => keyHandlers.push(handler),
  };
  const reports = [];
  const peers = new Set();
  const server = createServer(socket => {
    peers.add(socket);
    socket.on("close", () => peers.delete(socket));
    let data = "";
    socket.on("data", chunk => { data += chunk; });
    socket.on("end", () => { if (data) reports.push(JSON.parse(data)); });
  });
  const env = { KITTY_WINDOW_ID: "123", KITTY_LISTEN_ON: "unix:/tmp/kitty-test.sock", XDG_STATE_HOME: join(home, "state") };
  const platform = options.platform ?? "darwin";
  registerReporting({ on: (name, handler) => handlers.set(name, handler) }, {
    home, env, pid: 42, platform, runCommand: async () => "", ...options,
  });
  const uiContext = { ui, setWidget: ui.setWidget };
  const directory = ipcDirectory(home, platform);
  async function listen(at = directory) {
    await mkdir(at, { recursive: true });
    await new Promise((resolve, reject) => {
      server.once("error", reject);
      server.listen(join(at, "events.sock"), resolve);
    });
  }
  t.after(async () => {
    await handlers.get("session_shutdown")();
    for (const socket of peers) socket.destroy();
    if (server.listening) await new Promise(resolve => server.close(resolve));
    await rm(home, { recursive: true, force: true });
  });
  return {
    handlers, reports, listen, widgets, keyHandlers, uiContext, env, home,
    start: (idle = true) => handlers.get("session_start")({}, { cwd: "/tmp/project", isIdle: () => idle, ...uiContext }),
  };
}

// A file another Pi session would have published in the shared state directory.
function publishOther(t, f, { pid = 43, pending = true, pane = "777" } = {}) {
  const name = `pane-${pane}-${pid}.json`;
  mkdirSync(stateDir(f), { recursive: true });
  writeFileSync(join(stateDir(f), name), JSON.stringify({ version: 1, pid, pending, kitty_window_id: pane }));
  return name;
}

test("the IPC socket directory exists only where a companion app does", () => {
  const home = "/home/test-user";
  assert.equal(ipcDirectory(home, "darwin"), join(home, "Library/Application Support/Pi Watch"));
  assert.equal(ipcDirectory(home, "linux"), null);
  assert.equal(ipcDirectory(home, "win32"), null);
});

const fields = ["version", "id", "cwd", "pid", "kitty_window_id", "kitty_listen_on", "kitty_focused", "project_label", "kitty_title", "status"].sort();

test("lifecycle uses final settlement, reports only allowed fields, and shuts down idempotently", async t => {
  const f = await fixture(t);
  await f.listen();
  await f.start();
  await until(() => f.reports.length === 1);
  const protocol = JSON.parse(readFileSync(new URL("./fixtures/protocol-v1.json", import.meta.url), "utf8"));
  assert.deepEqual(f.reports[0], { ...protocol, id: f.reports[0].id });
  f.handlers.get("before_agent_start")({ prompt: "must not be reported" });
  await until(() => f.reports.length === 2);
  f.handlers.get("agent_start")({});
  await until(() => f.reports.length === 3);
  assert.equal(f.handlers.has("agent_end"), false);
  f.handlers.get("agent_settled")({ messages: ["must not be reported"] });
  await until(() => f.reports.length === 4);
  await f.handlers.get("session_shutdown")();
  await until(() => f.reports.length === 5);
  await f.handlers.get("session_shutdown")();
  assert.deepEqual(f.reports.map(r => r.status), ["idle", "working", "working", "idle", "disconnected"]);
  assert.equal(new Set(f.reports.map(r => r.id)).size, 1);
  for (const report of f.reports) {
    assert.deepEqual(Object.keys(report).sort(), fields);
    assert.equal(report.version, 1);
    assert.equal(report.pid, 42);
    assert.equal(report.cwd, "/tmp/project");
    assert.equal(report.kitty_window_id, "123");
    assert.equal(report.kitty_listen_on, "unix:/tmp/kitty-test.sock");
    assert.equal(report.kitty_focused, null);
  }
});

test("a platform without a companion app publishes state and sends no reports", async t => {
  const f = await fixture(t, { platform: "linux" });
  // A listener at the former Linux path must receive nothing.
  await f.listen(join(f.home, ".local/share/Pi Watch"));
  await f.start();
  f.handlers.get("agent_start")({});
  f.handlers.get("agent_settled")({}, f.uiContext);
  await pause(60);
  assert.equal(f.reports.length, 0, "no report is attempted without a companion app");
  assert.equal(ownState(f).pending, true, "state files still publish");
});

test("a missing app is harmless and heartbeat discovers an app started later", async t => {
  const f = await fixture(t, { heartbeatMs: 20 });
  await f.start();
  await pause(40);
  await f.listen();
  await until(() => f.reports.length > 0);
  assert.equal(f.reports[0].status, "idle");
});

test("startup respects Pi's current busy state and replacement changes reporting identity", async t => {
  const f = await fixture(t);
  await f.listen();
  await f.start(false);
  await until(() => f.reports.length === 1);
  assert.equal(f.reports[0].status, "working");
  await f.start();
  await until(() => f.reports.length === 2);
  assert.equal(f.reports[1].status, "idle");
  assert.notEqual(f.reports[0].id, f.reports[1].id);
  assert.equal(f.reports[0].pid, f.reports[1].pid);
});

test("metadata requests only the reporting pane and lifecycle reports do not reuse cached focus", async t => {
  const calls = [];
  const f = await fixture(t, { runCommand: async (file, args) => {
    calls.push([file, args]);
    if (file === "kitty") return JSON.stringify([{
      is_focused: true, tabs: [{ is_focused: true, windows: [{
        id: 123, title: "My pane", is_focused: true, env: { ignored: "private" },
      }] }],
    }]);
    if (args.includes("--show-toplevel")) return "/tmp/project";
    return "";
  } });
  await f.listen();
  await f.start();
  await until(() => f.reports.length === 1);
  assert.equal(f.reports[0].kitty_focused, true);
  assert.equal(f.reports[0].kitty_title, "My pane");
  assert.equal(f.reports[0].project_label, "project");
  assert.deepEqual(calls.find(([file]) => file === "kitty"), [
    "kitty", ["@", "--to", "unix:/tmp/kitty-test.sock", "ls", "--match", "id:123"],
  ]);
  f.handlers.get("agent_settled")({});
  await until(() => f.reports.length === 2);
  assert.equal(f.reports[1].kitty_focused, null);
  assert.deepEqual(Object.keys(f.reports[0]).sort(), fields);
});

test("the state directory follows XDG_STATE_HOME", () => {
  assert.equal(stateDirectory("/home/test-user", {}, "linux"), join("/home/test-user", ".local/state/pi-watch/sessions"));
  assert.equal(stateDirectory("/home/test-user", { XDG_STATE_HOME: "/custom/state" }, "darwin"), join("/custom/state", "pi-watch/sessions"));
  assert.equal(stateDirectory("/home/test-user", {}, "win32"), null);
});

const stateDir = f => join(f.home, "state", "pi-watch", "sessions");

// The reporting instance publishes with its own pid (42 in the fixture).
const ownState = f => JSON.parse(readFileSync(join(stateDir(f), readdirSync(stateDir(f)).find(file => JSON.parse(readFileSync(join(stateDir(f), file), "utf8")).pid === 42)), "utf8"));

function fixtureFiles(f) {
  try { return readdirSync(stateDir(f)); } catch { return null; }
}

async function deadPid() {
  const child = spawn("true");
  await once(child, "exit");
  return child.pid;
}

test("settlement publishes pending state and the next prompt or shutdown withdraws it", async t => {
  const f = await fixture(t);
  await f.listen();
  await f.start();
  assert.deepEqual(ownState(f), { version: 1, pid: 42, pending: false, kitty_window_id: "123" });
  f.handlers.get("agent_start")({});
  f.handlers.get("agent_settled")({});
  assert.equal(ownState(f).pending, true, "a settled run publishes pending");
  f.handlers.get("agent_settled")({});
  assert.equal(ownState(f).pending, true, "an idle re-settle keeps pending");
  f.handlers.get("before_agent_start")({}, f.uiContext);
  assert.equal(ownState(f).pending, false, "a new prompt withdraws pending");
  f.handlers.get("agent_settled")({});
  f.handlers.get("agent_settled")({});
  assert.equal(ownState(f).pending, true);
  await f.handlers.get("session_shutdown")();
  assert.equal(fixtureFiles(f).includes(""), false, "shutdown removes the state file");
});

test("the widget shows other pending sessions and navigation cycles their panes with ctrl+'", async t => {
  const kittyCalls = [];
  const f = await fixture(t, {
    runCommand: async (file, args) => {
      if (args.includes("focus-window")) kittyCalls.push(args);
      return "";
    },
    alive: () => true,
  });
  await f.listen();
  await f.start();
  publishOther(t, f, { pane: "777" });
  publishOther(t, f, { pane: "888", pid: 44 });
  f.handlers.get("before_agent_start")({}, f.uiContext);
  assert.deepEqual(f.widgets.get("pi-watch"), ["◇ 2 done elsewhere — ctrl+' switch"]);
  assert.equal(f.keyHandlers.length, 1, "the key handler installs once");
  const press = input => f.keyHandlers[0](input);
  assert.equal(press("\x1b[39;5u").consume, true, "CSI ctrl+' is consumed while others await");
  assert.equal(kittyCalls.at(-1).includes("id:777"), true, "navigation calls kitty with the pane id");
  assert.equal(press("\x07").consume, true, "BEL ctrl+' is consumed while others await");
  assert.equal(kittyCalls.at(-1).includes("id:888"), true, "rotation advances across panes");
  assert.deepEqual(f.widgets.get("pi-watch"), ["◇ 2 done elsewhere — ctrl+' switch"], "navigation does not withdraw files it does not own");
  assert.equal(press("x"), undefined, "ordinary keys pass through");
  f.handlers.get("before_agent_start")({}, f.uiContext);
  assert.equal(press("\x07").consume, true);
  assert.ok(kittyCalls.at(-1).some(a => a.startsWith("id:")), "the key navigates with a pane id");
  publishOther(t, f, { pane: "999", pid: 45 });
  f.handlers.get("before_agent_start")({}, f.uiContext);
  assert.deepEqual(f.widgets.get("pi-watch"), ["◇ 3 done elsewhere — ctrl+' switch"], "the count tracks the directory");
  for (let i = 0; i < 3; i += 1) {
    assert.equal(press(i === 2 ? "\x1b[39;6u" : "\x07").consume, true);
  }
  assert.ok(kittyCalls.slice(-3).some(args => args.at(-1) === "id:999"), "rotation cycles through every listed pane");
  assert.equal(f.keyHandlers.length, 1);
});

test("the widget disappears when no other session awaits and stale states are swept on the next read", async t => {
  const stale = await deadPid();
  const f = await fixture(t, { heartbeatMs: 20 });
  await f.listen();
  await f.start();
  const other = publishOther(t, f, { pane: "777", pid: process.pid });
  const staleFile = publishOther(t, f, { pane: "888", pid: stale });
  f.handlers.get("before_agent_start")({}, f.uiContext);
  assert.deepEqual(f.widgets.get("pi-watch"), ["◇ 1 done elsewhere — ctrl+' switch"], "only a live pid counts");
  await rm(join(stateDir(f), other));
  f.handlers.get("before_agent_start")({}, f.uiContext);
  assert.equal(f.widgets.get("pi-watch"), undefined);
  assert.equal(existsSync(join(stateDir(f), staleFile)), false, "a dead pid's file is swept");
});

test("the widget stays silent without a ui context while state files still publish", async t => {
  const f = await fixture(t);
  await f.listen();
  await f.start();
  publishOther(t, f, { pane: "777" });
  f.handlers.get("agent_start")({});
  f.handlers.get("agent_settled")({});
  assert.deepEqual(f.widgets.size, 0);
  assert.equal(ownState(f).pending, true, "publishing works without ui");
  await f.handlers.get("session_shutdown")();
  assert.equal(fixtureFiles(f).includes(""), false);
});

test("reporting without Kitty retains fallback labels and does not query Kitty", async t => {
  const f = await fixture(t, { env: {}, runCommand: async file => {
    assert.notEqual(file, "kitty");
    return "";
  } });
  await f.listen();
  await f.start();
  await until(() => f.reports.length === 1);
  assert.equal(f.reports[0].kitty_window_id, null);
  assert.equal(f.reports[0].kitty_listen_on, null);
  assert.equal(f.reports[0].kitty_focused, null);
  assert.equal(f.reports[0].project_label, "project");
});

test("previews include only bounded completed assistant text and reset on replacement", async t => {
  const f = await fixture(t);
  await f.listen();
  await f.start();
  await until(() => f.reports.length === 1);
  const end = message => f.handlers.get("message_end")({ message });
  end({ role: "user", content: [{ type: "text", text: "private" }] });
  end({ role: "assistant", stopReason: "aborted", content: [{ type: "text", text: "partial" }] });
  await pause(20);
  assert.equal(f.reports.length, 1);
  end({ role: "assistant", stopReason: "stop", content: [
    { type: "thinking", thinking: "private" },
    { type: "toolCall", arguments: { secret: "private" } },
    { type: "text", text: "A".repeat(240) + "omitted middle" + "😀".repeat(240) },
  ] });
  await until(() => f.reports.length === 2);
  assert.deepEqual(f.reports.at(-1).preview, { first: "A".repeat(240), last: "😀".repeat(240) });
  f.handlers.get("agent_start")({});
  await until(() => f.reports.length === 3);
  assert.deepEqual(f.reports.at(-1).preview, f.reports[1].preview);
  await f.start();
  await until(() => f.reports.length === 4);
  assert.equal(f.reports.at(-1).preview, undefined);
});

test("preview extraction normalizes text and avoids overlap", () => {
  assert.equal(responsePreview([{ type: "thinking", thinking: "secret" }]), null);
  assert.deepEqual(responsePreview([{ type: "text", text: " hello\n\tworld\u0000 " }]), { first: "hello world", last: "" });
  assert.deepEqual(responsePreview([{ type: "text", text: "a".repeat(240) + "end" }]), { first: "a".repeat(240), last: "end" });
});

test("shutdown stops heartbeat reporting", async t => {
  const f = await fixture(t, { heartbeatMs: 20, alive: () => true });
  await f.listen();
  await f.start();
  await until(() => f.reports.length >= 2);
  await f.handlers.get("session_shutdown")();
  await until(() => f.reports.at(-1)?.status === "disconnected");
  const count = f.reports.length;
  await pause(80);
  assert.equal(f.reports.length, count);
});
