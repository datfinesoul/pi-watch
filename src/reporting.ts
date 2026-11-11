/** Cross-session completion and input-waiting markers, without transcript access. */
import { homedir } from "node:os";
import { mkdirSync, readdirSync, readFileSync, renameSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { randomUUID } from "node:crypto";
import { paneFocused } from "./metadata.ts";

const execute = promisify(execFile);
async function command(file, args, cwd) {
  try {
    const { stdout } = await execute(file, args, { cwd, timeout: 1000, maxBuffer: 128 * 1024 });
    return stdout.trim();
  } catch { return ""; }
}

// Runtime state, not user configuration. Keep the existing location for upgrades.
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
  const dir = stateDirectory(home, env, platform);
  let id = "";
  let cwd = "";
  let working = false;
  let pending = false;
  let waiting = false;
  let previousFocus = null;
  let ui = null;
  let unsubscribeKeys;
  let others = [];
  let rotation = 0;
  let timer;
  const pane = env.KITTY_WINDOW_ID;
  const endpoint = env.KITTY_LISTEN_ON;
  const navigable = typeof pane === "string" && /^\d+$/.test(pane) && endpoint?.startsWith("unix:/");

  function publish() {
    if (!dir || !id) return;
    const path = join(dir, `${id}.json`);
    const temporary = `${path}.tmp`;
    try {
      mkdirSync(dir, { recursive: true, mode: 0o700 });
      // Atomic replacement prevents readers from seeing a partially written file.
      writeFileSync(temporary, JSON.stringify({ version: 1, pid, pending, waiting, kitty_window_id: pane ?? null }), { mode: 0o600 });
      renameSync(temporary, path);
    } catch { /* Runtime state is best effort. */ }
    finally { try { rmSync(temporary, { force: true }); } catch { /* Best effort. */ } }
  }
  function unpublish() {
    if (!dir || !id) return;
    try { rmSync(join(dir, `${id}.json`), { force: true }); } catch { /* Best effort. */ }
  }
  function refreshWidget(ctx?) {
    if (ctx?.hasUI && ctx.mode === "tui" && ctx.ui && ui !== ctx.ui) {
      if (typeof unsubscribeKeys === "function") unsubscribeKeys();
      ui = ctx.ui;
      unsubscribeKeys = ui.onTerminalInput?.(data => {
        if (!/^(\x07|\x1b\[27;5;39~|\x1b\[39;6u|\x1b\[39;5u)$/.test(data)) return;
        if (!id || !navigable || !others.length) return;
        const target = others[rotation % others.length];
        rotation = (rotation + 1) % others.length;
        void runCommand("kitty", ["@", "--to", endpoint, "focus-window", "--match", `id:${target.pane}`], cwd);
        return { consume: true };
      });
    }
    others = [];
    if (dir && id && navigable) {
      try {
        others = readdirSync(dir).sort().flatMap(file => {
          if (!file.endsWith(".json") || file === `${id}.json`) return [];
          try {
            const state = JSON.parse(readFileSync(join(dir, file), "utf8"));
            if (state?.version !== 1 || !Number.isSafeInteger(state.pid) || state.pid <= 0) return [];
            if (!alive(state.pid)) { try { rmSync(join(dir, file)); } catch { /* Gone already. */ } return []; }
            if (state.pid === pid || (state.pending !== true && state.waiting !== true)) return [];
            if (typeof state.kitty_window_id !== "string" || !/^\d+$/.test(state.kitty_window_id)) return [];
            if (state.kitty_window_id === pane) return [];
            return [{ pane: state.kitty_window_id, waiting: state.waiting === true }];
          } catch { return []; }
        });
      } catch { /* No shared state yet. */ }
    }
    // Input requests take priority, but repeated shortcut presses visit every pane.
    others.sort((a, b) => Number(b.waiting) - Number(a.waiting));
    if (rotation >= others.length) rotation = 0;
    const inputCount = others.filter(other => other.waiting).length;
    const doneCount = others.length - inputCount;
    const parts = [];
    if (inputCount) parts.push(`${inputCount} input`);
    if (doneCount) parts.push(`${doneCount} done`);
    try { ui?.setWidget("pi-watch", parts.length ? [`◇ ${parts.join(" · ")}  [ctrl+' switch]`] : undefined); } catch { /* No usable UI. */ }
  }
  async function heartbeat() {
    const instance = id;
    if (!instance) return;
    refreshWidget();
    if (!navigable) return;
    const output = await runCommand("kitty", ["@", "--to", endpoint, "ls", "--match", `id:${pane}`], cwd);
    if (instance !== id) return;
    const focused = paneFocused(output, pane);
    // A visit acknowledges completion, never an unanswered prompt.
    if (pending && previousFocus === false && focused === true) {
      pending = false;
      publish();
    }
    previousFocus = focused;
  }
  function stop() {
    if (timer) clearInterval(timer);
    timer = undefined;
    if (typeof unsubscribeKeys === "function") unsubscribeKeys();
    unsubscribeKeys = undefined;
    try { ui?.setWidget("pi-watch", undefined); } catch { /* UI already disposed. */ }
    ui = null;
    others = [];
    rotation = 0;
    unpublish();
    id = "";
    working = pending = waiting = false;
    previousFocus = null;
  }
  pi.on("session_start", async (_event, ctx) => {
    stop();
    id = randomUUID();
    cwd = ctx.cwd;
    working = !ctx.isIdle();
    publish();
    refreshWidget(ctx);
    const instance = id;
    await heartbeat();
    if (instance !== id) return;
    // Serialize focus polling so a slow Kitty command cannot reorder samples.
    let polling = false;
    timer = setInterval(() => {
      if (polling) return;
      polling = true;
      void heartbeat().catch(() => {}).finally(() => { polling = false; });
    }, heartbeatMs);
    timer.unref();
  });
  function startWork(_event, ctx) {
    if (!id) return;
    working = true;
    pending = false;
    publish();
    refreshWidget(ctx);
  }
  pi.on("before_agent_start", startWork);
  pi.on("agent_start", startWork);
  // agent_end is NOT idle: retries and queued work can follow it.
  pi.on("agent_settled", (_event, ctx) => {
    if (!id) return;
    pending = working || pending;
    working = false;
    publish();
    refreshWidget(ctx);
  });
  // Pi coalesces nested/overlapping prompts into one outer waiting span.
  pi.on("ui_prompt_start", (_event, ctx) => {
    if (!id) return;
    waiting = true;
    publish();
    refreshWidget(ctx);
  });
  pi.on("ui_prompt_end", (_event, ctx) => {
    if (!id) return;
    waiting = false;
    publish();
    refreshWidget(ctx);
  });
  pi.on("session_shutdown", stop);
}
