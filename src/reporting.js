/** Minimal local status reporting. No prompts, transcripts, or tool data. */
import { createConnection } from "node:net";
import { homedir } from "node:os";
import { join } from "node:path";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { randomUUID } from "node:crypto";
import { paneMetadata, projectLabel } from "./metadata.js";

const execute = promisify(execFile);
async function command(file, args, cwd) {
  try {
    const { stdout } = await execute(file, args, { cwd, timeout: 1000, maxBuffer: 128 * 1024 });
    return stdout.trim();
  } catch { return ""; }
}

// Overrides are test seams, not user configuration.
export function registerReporting(pi, {
  home = homedir(), env = process.env, pid = process.pid,
  runCommand = command, heartbeatMs = 5000,
} = {}) {
  const path = join(home, "Library/Application Support/Pi Watch/events.sock");
  let id = "";
  let cwd = "";
  let project = null;
  let title = null;
  let focused = null;
  let refreshing = false;
  async function refreshMetadata() {
    if (refreshing) return;
    refreshing = true;
    const instance = id;
    const directory = cwd;
    try {
      const [root, superRoot] = await Promise.all([
        runCommand("git", ["rev-parse", "--show-toplevel"], directory),
        runCommand("git", ["rev-parse", "--show-superproject-working-tree"], directory),
      ]);
      const submodules = root ? await runCommand("git", ["config", "--file", join(root, ".gitmodules"), "--get-regexp", "^submodule\\..*\\.path$"], directory) : "";
      const pane = env.KITTY_WINDOW_ID;
      const endpoint = env.KITTY_LISTEN_ON;
      let next = { title: null, focused: null };
      if (pane && /^\d+$/.test(pane) && endpoint?.startsWith("unix:/")) {
        // Request only this pane. Retain only its title and focus flags.
        const output = await runCommand("kitty", ["@", "--to", endpoint, "ls", "--match", `id:${pane}`], directory);
        next = paneMetadata(output, pane);
      }
      if (instance === id) {
        project = projectLabel(directory, root, superRoot, !!submodules);
        title = next.title;
        focused = next.focused;
      }
    } finally { refreshing = false; }
  }
  let status = "idle";
  let timer;
  const sockets = new Set();
  function report(includeFocus = false) {
    if (!id || sockets.size >= 2) return;
    const socket = createConnection({ path });
    sockets.add(socket);
    socket.setTimeout(500, () => socket.destroy());
    socket.on("error", () => socket.destroy());
    socket.on("close", () => sockets.delete(socket));
    socket.on("connect", () => socket.end(JSON.stringify({
      version: 1, id, cwd, pid, project_label: project, kitty_title: title,
      kitty_window_id: env.KITTY_WINDOW_ID ?? null,
      kitty_listen_on: env.KITTY_LISTEN_ON ?? null,
      kitty_focused: includeFocus ? focused : null, status,
    })));
    socket.unref();
  }
  function stop() {
    if (timer) clearInterval(timer);
    timer = undefined;
    for (const socket of sockets) socket.destroy();
    sockets.clear();
  }
  async function start(ctx) {
    stop();
    id = randomUUID(); // A reporting instance, not a transcript identifier.
    cwd = ctx.cwd;
    status = ctx.isIdle() ? "idle" : "working";
    project = null;
    title = null;
    focused = null;
    const instance = id;
    await refreshMetadata();
    if (instance !== id) return;
    report(true);
    timer = setInterval(() => {
      void refreshMetadata().then(() => { if (instance === id) report(true); });
    }, heartbeatMs);
    timer.unref();
  }
  pi.on("session_start", (_event, ctx) => start(ctx));
  pi.on("before_agent_start", () => { status = "working"; report(); });
  pi.on("agent_start", () => { status = "working"; report(); });
  // agent_end is NOT idle: retries and queued work can follow it.
  pi.on("agent_settled", () => { status = "idle"; report(); });
  pi.on("session_shutdown", async () => {
    stop();
    status = "disconnected";
    report();
    // Best effort, bounded flush; abrupt process exits are caught by heartbeat expiry.
    await new Promise(resolve => setTimeout(resolve, 50));
    stop();
    id = "";
  });
}
