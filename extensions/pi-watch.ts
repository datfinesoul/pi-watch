/** Minimal local status reporting. No prompts, transcripts, or tool data. */
import type { ExtensionAPI, ExtensionContext } from "@earendil-works/pi-coding-agent";
import { createConnection } from "node:net";
import { homedir } from "node:os";
import { join } from "node:path";
import { randomUUID } from "node:crypto";

export default function (pi: ExtensionAPI) {
  const path = join(homedir(), "Library/Application Support/Pi Watch/events.sock");
  let id = "";
  let cwd = "";
  let status: "working" | "idle" | "disconnected" = "idle";
  let timer: ReturnType<typeof setInterval> | undefined;
  const sockets = new Set<ReturnType<typeof createConnection>>();
  function report() {
    if (!id || sockets.size >= 2) return;
    const socket = createConnection({ path });
    sockets.add(socket);
    socket.setTimeout(500, () => socket.destroy());
    socket.on("error", () => socket.destroy());
    socket.on("close", () => sockets.delete(socket));
    socket.on("connect", () => socket.end(JSON.stringify({
      version: 1, id, cwd, pid: process.pid,
      kitty_window_id: process.env.KITTY_WINDOW_ID ?? null, status,
    })));
    socket.unref();
  }
  function stop() {
    if (timer) clearInterval(timer);
    timer = undefined;
    for (const socket of sockets) socket.destroy();
    sockets.clear();
  }
  function start(ctx: ExtensionContext) {
    stop();
    id = randomUUID(); // A reporting instance, not a transcript identifier.
    cwd = ctx.cwd;
    status = ctx.isIdle() ? "idle" : "working";
    report();
    timer = setInterval(report, 5000);
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
