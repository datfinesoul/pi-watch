import assert from "node:assert/strict";
import { createServer } from "node:net";
import { mkdtemp, mkdir, rm } from "node:fs/promises";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { setTimeout as pause } from "node:timers/promises";
import test from "node:test";
import { ipcDirectory, registerReporting, responsePreview } from "../src/reporting.js";

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
  const reports = [];
  const peers = new Set();
  const server = createServer(socket => {
    peers.add(socket);
    socket.on("close", () => peers.delete(socket));
    let data = "";
    socket.on("data", chunk => { data += chunk; });
    socket.on("end", () => reports.push(JSON.parse(data)));
  });
  const env = { KITTY_WINDOW_ID: "123", KITTY_LISTEN_ON: "unix:/tmp/kitty-test.sock" };
  registerReporting({ on: (name, handler) => handlers.set(name, handler) }, {
    home, env, pid: 42, runCommand: async () => "", ...options,
  });
  const directory = ipcDirectory(home, env);
  async function listen() {
    await mkdir(directory, { recursive: true });
    await new Promise((resolve, reject) => {
      server.once("error", reject);
      server.listen(join(directory, "events.sock"), resolve);
    });
  }
  t.after(async () => {
    await handlers.get("session_shutdown")();
    for (const socket of peers) socket.destroy();
    if (server.listening) await new Promise(resolve => server.close(resolve));
    await rm(home, { recursive: true, force: true });
  });
  return {
    handlers, reports, listen,
    start: (idle = true) => handlers.get("session_start")({}, { cwd: "/tmp/project", isIdle: () => idle }),
  };
}

test("the IPC socket directory follows the platform", () => {
  const home = "/home/test-user";
  assert.equal(ipcDirectory(home, {}, "darwin"), join(home, "Library/Application Support/Pi Watch"));
  assert.equal(ipcDirectory(home, {}, "linux"), join(home, ".local/share/Pi Watch"));
  assert.equal(ipcDirectory(home, { XDG_DATA_HOME: "/custom/data" }, "linux"), join("/custom/data", "Pi Watch"));
  // The app ignores a non-absolute XDG_DATA_HOME, like the Rust app does.
  assert.equal(ipcDirectory(home, { XDG_DATA_HOME: "relative" }, "linux"), join(home, ".local/share/Pi Watch"));
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
  const f = await fixture(t, { heartbeatMs: 20 });
  await f.listen();
  await f.start();
  await until(() => f.reports.length >= 2);
  await f.handlers.get("session_shutdown")();
  await until(() => f.reports.at(-1)?.status === "disconnected");
  const count = f.reports.length;
  await pause(80);
  assert.equal(f.reports.length, count);
});
