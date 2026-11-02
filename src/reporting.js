/** Local status reporting with bounded, memory-only assistant previews. */
import { createConnection } from "node:net";
import { homedir } from "node:os";
import { mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { randomUUID } from "node:crypto";
import { paneMetadata, projectLabel } from "./metadata.js";

// Retain at most 240 Unicode code points at each end, never the full response.
export function responsePreview(content) {
  let first = "";
  let last = "";
  let length = 0;
  for (const block of content ?? []) {
    if (block.type !== "text" || typeof block.text !== "string") continue;
    for (const char of `${length ? " " : ""}${block.text}`) {
      if (/\s/u.test(char)) {
        if (!length || last.endsWith(" ")) continue;
      } else if (/\p{Cc}|\p{Cf}/u.test(char)) continue;
      const clean = /\s/u.test(char) ? " " : char;
      if (length < 240) first += clean;
      last = Array.from(last + clean).slice(-240).join("");
      length++;
    }
  }
  if (!first.trim()) return null;
  return { first: first.trim(), last: length > 240 ? Array.from(last).slice(-Math.min(240, length - 240)).join("").trim() : "" };
}

const execute = promisify(execFile);

// The directory must match the companion app built for this platform.
// Reports exist only where a companion app does, which is macOS only.
export function ipcDirectory(home, platform = process.platform) {
  if (platform !== "darwin") return null;
  return join(home, "Library/Application Support/Pi Watch");
}
async function command(file, args, cwd) {
  try {
    const { stdout } = await execute(file, args, { cwd, timeout: 1000, maxBuffer: 128 * 1024 });
    return stdout.trim();
  } catch { return ""; }
}

// Per-session one-file state lets a session render and navigate to other
// sessions that await inspection without aggregation or a central process.
// Contents are inert: version, pid, a pending flag, and a Kitty window ID.
export function stateDirectory(home, env = {}, platform = process.platform) {
  if (platform !== "linux" && platform !== "darwin") return null;
  const data = env.XDG_STATE_HOME;
  const base = typeof data === "string" && data.startsWith("/") ? data : join(home, ".local/state");
  return join(base, "pi-watch", "sessions");
}

// Overrides are test seams, not user configuration.
export function registerReporting(pi, {
  home = homedir(), env = process.env, pid = process.pid, platform = process.platform,
  runCommand = command, heartbeatMs = 5000,
  alive = pid => { try { process.kill(pid, 0); return true; } catch { return false; } },
} = {}) {
  const appDirectory = ipcDirectory(home, platform);
  const dir = stateDirectory(home, env, platform);
  let id = "";
  let cwd = "";
  let project = null;
  let title = null;
  let focused = null;
  let refreshing = false;
  const pendingWidgetKey = "pi-watch";
  let pendingUi = null;
  let pendingOthers = [];
  let pendingRotation = 0;
  function readPendingOthersList() {
    if (!dir) { pendingOthers = []; return; }
    try {
      pendingOthers = readdirSync(dir).flatMap(file => {
        if (!file.endsWith(".json") || file === `${id}.json`) return [];
        try {
          const state = JSON.parse(readFileSync(join(dir, file), "utf8"));
          if (state?.version !== 1) return [];
          if (state.pending !== true || typeof state.pid !== "number" || state.pid <= 0) return [];
          if (typeof state.kitty_window_id !== "string" || !/^\d+$/.test(state.kitty_window_id)) return [];
          if (!alive(state.pid)) { try { rmSync(join(dir, file)); } catch { /* Gone already. */ } return []; }
          return [{ pane: state.kitty_window_id, file }];
        } catch { return []; }
      });
    } catch { pendingOthers = []; }
    if (pendingRotation >= pendingOthers.length) pendingRotation = 0;
  }
  function syncWidget() {
    const count = pendingOthers.length;
    try { pendingUi?.setWidget(pendingWidgetKey, count ? [`◇ ${count} done elsewhere — ctrl+' switch`] : undefined); } catch { /* No usable UI in this mode. */ }
  }
  function refreshWidget(ctx) {
    if (ctx?.ui?.setWidget && pendingUi !== ctx.ui) {
      pendingUi = ctx.ui;
      watchKeys(pendingUi);
    }
    readPendingOthersList();
    syncWidget();
  }
  function publishPending(value) {
    if (!dir || !id) return;
    try {
      try { mkdirSync(dir, { recursive: true, mode: 0o700 }); } catch { /* Locked by a concurrent writer. */ }
      writeFileSync(join(dir, `${id}.json`), JSON.stringify({ version: 1, pid, pending: value, kitty_window_id: env.KITTY_WINDOW_ID ?? null }), { mode: 0o600 });
    } catch { /* State files are best effort. */ }
  }
  function unpublish() {
    if (!dir || !id) return;
    try { rmSync(join(dir, `${id}.json`), { force: true }); } catch { /* Best effort. */ }
  }
  let keysInstalled = false;
  function watchKeys(ui) {
    if (keysInstalled || typeof ui.onTerminalInput !== "function") return;
    keysInstalled = true;
    // Ctrl+' arrives as BEL on legacy terminals and as CSI 39;5u (Ctrl)
    // or CSI 39;6u (Ctrl+Shift) under the Kitty keyboard protocol.
    ui.onTerminalInput(data => {
      if (!/^(\x07|\x1b\[27;5;39~|\x1b\[39;6u|\x1b\[39;5u)$/.test(data)) return;
      if (!pendingOthers.length) return;
      const target = pendingOthers[pendingRotation % pendingOthers.length];
      pendingRotation = (pendingRotation + 1) % pendingOthers.length;
      void runCommand("kitty", ["@", "--to", env.KITTY_LISTEN_ON ?? "", "focus-window", "--match", `id:${target.pane}`], cwd);
      return { consume: true };
    });
  }
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
        const reached = previousFocus === false && next.focused === true;
        previousFocus = next.focused;
        focused = next.focused;
        // Visiting the pane counts as inspection, matching the app. Only a
        // change from unfocused to focused after the run ended counts, so a
        // pane that was already focused when it settled stays pending.
        if (pending && reached) {
          pending = false;
          publishPending(false);
        }
      }
    } finally { refreshing = false; }
  }
  // Pending means this session ended a run and nobody inspected it yet.
  // The marker is a quiet one-line widget; it never takes focus or keys.
  let pending = false;
  // Focus is sampled on each heartbeat. Only a change from unfocused to
  // focused shows that the user reached the pane after the run ended.
  let previousFocus = null;
  let preview = null;
  let status = "idle";
  let timer;
  const sockets = new Set();
  function report(includeFocus = false) {
    if (!id || !appDirectory || sockets.size >= 2) return;
    const socket = createConnection({ path: join(appDirectory, "events.sock") });
    sockets.add(socket);
    socket.setTimeout(500, () => socket.destroy());
    socket.on("error", () => socket.destroy());
    socket.on("close", () => sockets.delete(socket));
    socket.on("connect", () => socket.end(JSON.stringify({
      version: 1, id, cwd, pid, project_label: project, kitty_title: title,
      kitty_window_id: env.KITTY_WINDOW_ID ?? null,
      kitty_listen_on: env.KITTY_LISTEN_ON ?? null,
      kitty_focused: includeFocus ? focused : null, status,
      ...(preview ? { preview } : {}),
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
    preview = null;
    project = null;
    title = null;
    focused = null;
    previousFocus = null;
    refreshWidget(ctx);
    publishPending(false);
    const instance = id;
    await refreshMetadata();
    if (instance !== id) return;
    report(true);
    timer = setInterval(() => {
      void refreshMetadata().then(() => {
        if (instance === id) { refreshWidget(); report(true); }
      });
    }, heartbeatMs);
    timer.unref();
  }
  pi.on("session_start", (_event, ctx) => start(ctx));
  pi.on("before_agent_start", (_event, ctx) => {
    refreshWidget(ctx);
    status = "working";
    publishPending(false); pending = false;
    report();
  });
  pi.on("agent_start", () => { status = "working"; report(); });
  pi.on("message_end", ({ message }) => {
    if (message.role !== "assistant" || message.stopReason !== "stop") return;
    const next = responsePreview(message.content);
    if (next) { preview = next; report(); }
  });
  // agent_end is NOT idle: retries and queued work can follow it.
  pi.on("agent_settled", (_event, ctx) => {
    refreshWidget(ctx);
    // Only a run that actually worked leaves this session pending; an idle
    // re-settle keeps an existing pending flag.
    pending = status === "working" ? true : pending;
    publishPending(pending);
    status = "idle";
    report();
  });
  pi.on("session_shutdown", async () => {
    stop();
    unpublish();
    pending = false;
    status = "disconnected";
    report();
    // Best effort, bounded flush; abrupt process exits are caught by heartbeat expiry.
    await new Promise(resolve => setTimeout(resolve, 50));
    stop();
    id = "";
  });
}
