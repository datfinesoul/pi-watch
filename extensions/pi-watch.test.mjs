import assert from "node:assert/strict";
import { createServer } from "node:net";
import { mkdtemp, mkdir, rm } from "node:fs/promises";
import { join } from "node:path";
import extension, { projectLabel, paneTitle } from "./pi-watch.ts";

assert.equal(projectLabel("/work/super/repo/src", "/work/super/repo", "/work/super", false), "↳/super/repo/src");
assert.equal(projectLabel("/work/super", "/work/super", "", true), "↳/super");
assert.equal(projectLabel("/work/repo/src", "/work/repo", "", false), "repo/src");
assert.equal(projectLabel("/work/plain", "", "", false), "plain");
assert.equal(paneTitle(JSON.stringify([{ tabs: [{ windows: [{ id: 123, title: "My pane", env: { ignored: true } }] }] }]), "123"), "My pane");
assert.equal(paneTitle("invalid", "123"), null);
assert.equal(paneTitle("[]", "123"), null);

const pause = () => new Promise(resolve => setTimeout(resolve, 80));

// Standalone lifecycle/IPC harness.
await (async () => {
  const home = await mkdtemp("/tmp/pw-");
  const oldHome = process.env.HOME;
  const oldPane = process.env.KITTY_WINDOW_ID;
  const oldEndpoint = process.env.KITTY_LISTEN_ON;
  process.env.KITTY_LISTEN_ON = "unix:/tmp/kitty-test.sock";
  process.env.HOME = home;
  process.env.KITTY_WINDOW_ID = "123";
  const handlers = new Map();
  extension({ on: (name, handler) => handlers.set(name, handler) });
  const reports = [];
  const peers = new Set();
  const server = createServer(socket => {
    peers.add(socket);
    socket.on("close", () => peers.delete(socket));
    let data = "";
    socket.on("data", chunk => { data += chunk; });
    socket.on("end", () => reports.push(JSON.parse(data)));
  });
  try {
    // Missing app must not reject a Pi lifecycle handler.
    await handlers.get("session_start")({}, { cwd: "/tmp/project", isIdle: () => true });
    await pause();
    const dir = join(home, "Library/Application Support/Pi Watch");
    await mkdir(dir, { recursive: true });
    await new Promise(resolve => server.listen(join(dir, "events.sock"), resolve));
    handlers.get("agent_start")({ prompt: "must not be reported" });
    await pause();
    handlers.get("agent_settled")({ messages: ["must not be reported"] });
    await pause();
    await handlers.get("session_shutdown")();
    await pause();
    assert.deepEqual(reports.map(r => r.status), ["working", "idle", "disconnected"]);
    assert.equal(reports[0].kitty_window_id, "123");
    assert.equal(reports[0].kitty_listen_on, "unix:/tmp/kitty-test.sock");
    assert.equal(reports[0].cwd, "/tmp/project");
    assert.deepEqual(Object.keys(reports[0]).sort(), ["version", "id", "cwd", "pid", "kitty_window_id", "kitty_listen_on", "project_label", "kitty_title", "status"].sort());
    assert.equal(new Set(reports.map(r => r.id)).size, 1);
    assert.equal(handlers.has("agent_end"), false);
  } finally {
    await handlers.get("session_shutdown")();
    for (const socket of peers) socket.destroy();
    if (server.listening) server.close();
    if (oldHome === undefined) delete process.env.HOME; else process.env.HOME = oldHome;
    if (oldEndpoint === undefined) delete process.env.KITTY_LISTEN_ON; else process.env.KITTY_LISTEN_ON = oldEndpoint;
    if (oldPane === undefined) delete process.env.KITTY_WINDOW_ID; else process.env.KITTY_WINDOW_ID = oldPane;
    await rm(home, { recursive: true, force: true });
  }
})();
console.log("Extension lifecycle/IPC check passed");
