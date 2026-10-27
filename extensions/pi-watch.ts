/** Minimal local status reporting. No prompts, transcripts, or tool data. */
import type { ExtensionAPI, ExtensionContext } from "@earendil-works/pi-coding-agent";
import { createConnection } from "node:net";
import { homedir } from "node:os";
import { basename, join, relative } from "node:path";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { randomUUID } from "node:crypto";

const execute = promisify(execFile);
async function command(file: string, args: string[], cwd: string) {
  try {
    const { stdout } = await execute(file, args, { cwd, timeout: 1000, maxBuffer: 128 * 1024 });
    return stdout.trim();
  } catch { return ""; }
}

export function projectLabel(cwd: string, root: string, superRoot: string, hasSubmodules: boolean) {
  const base = superRoot || root;
  if (!base) return basename(cwd) || cwd;
  const suffix = relative(base, cwd);
  return `${superRoot || hasSubmodules ? "↳/" : ""}${basename(base)}${suffix ? `/${suffix}` : ""}`;
}

export function paneMetadata(output: string, pane: string): { title: string | null; focused: boolean | null } {
  try {
    for (const window of JSON.parse(output)) {
      for (const tab of window.tabs ?? []) {
        for (const candidate of tab.windows ?? []) {
          if (String(candidate.id) === pane) {
            const flags = [window.is_focused, tab.is_focused, candidate.is_focused];
            return {
              title: typeof candidate.title === "string" ? candidate.title.replace(/[\u0000-\u001f\u007f]/g, "").slice(0, 200) || null : null,
              focused: flags.every(flag => typeof flag === "boolean") ? flags.every(flag => flag) : null,
            };
          }
        }
      }
    }
  } catch { /* Missing or malformed Kitty response. */ }
  return { title: null, focused: null };
}

export function paneTitle(output: string, pane: string): string | null {
  return paneMetadata(output, pane).title;
}

export default function (pi: ExtensionAPI) {
  const path = join(homedir(), "Library/Application Support/Pi Watch/events.sock");
  let id = "";
  let cwd = "";
  let project: string | null = null;
  let title: string | null = null;
  let focused: boolean | null = null;
  let refreshing = false;
  async function refreshMetadata() {
    if (refreshing) return;
    refreshing = true;
    const instance = id;
    const directory = cwd;
    try {
      const [root, superRoot] = await Promise.all([
        command("git", ["rev-parse", "--show-toplevel"], directory),
        command("git", ["rev-parse", "--show-superproject-working-tree"], directory),
      ]);
      const submodules = root ? await command("git", ["config", "--file", join(root, ".gitmodules"), "--get-regexp", "^submodule\\..*\\.path$"], directory) : "";
      const pane = process.env.KITTY_WINDOW_ID;
      const endpoint = process.env.KITTY_LISTEN_ON;
      let next = { title: null, focused: null } as ReturnType<typeof paneMetadata>;
      if (pane && /^\d+$/.test(pane) && endpoint?.startsWith("unix:/")) {
        // Request only this pane. Retain only its title and focus flags.
        const output = await command("kitty", ["@", "--to", endpoint, "ls", "--match", `id:${pane}`], directory);
        next = paneMetadata(output, pane);
      }
      if (instance === id) {
        project = projectLabel(directory, root, superRoot, !!submodules);
        title = next.title;
        focused = next.focused;
      }
    } finally { refreshing = false; }
  }
  let status: "working" | "idle" | "disconnected" = "idle";
  let timer: ReturnType<typeof setInterval> | undefined;
  const sockets = new Set<ReturnType<typeof createConnection>>();
  function report(includeFocus = false) {
    if (!id || sockets.size >= 2) return;
    const socket = createConnection({ path });
    sockets.add(socket);
    socket.setTimeout(500, () => socket.destroy());
    socket.on("error", () => socket.destroy());
    socket.on("close", () => sockets.delete(socket));
    socket.on("connect", () => socket.end(JSON.stringify({
      version: 1, id, cwd, pid: process.pid, project_label: project, kitty_title: title,
      kitty_window_id: process.env.KITTY_WINDOW_ID ?? null,
      kitty_listen_on: process.env.KITTY_LISTEN_ON ?? null,
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
  async function start(ctx: ExtensionContext) {
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
    }, 5000);
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
    await new Promise<void>((resolve) => setTimeout(resolve, 50));
    stop();
    id = "";
  });
}
