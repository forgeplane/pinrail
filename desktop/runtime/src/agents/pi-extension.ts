// Pinrail's extension for Pi, installed by Pinrail when Report status is on
// for Pi, and removed when it is turned off: Pinrail rewrites this file, so
// add your own extensions beside it instead of editing it. It reports the
// session's state to Pinrail by running `pinrail hook pi <event>` with JSON
// on its input, as the hooks of other agents do.
import { spawn, spawnSync } from "node:child_process";

const cli = process.env.PINRAIL_CLI || "pinrail";

/** The part of Pi's extension API this extension uses. */
interface Context {
  cwd: string;
  sessionManager: { getSessionId(): string; getSessionFile(): string | null | undefined };
}
interface Message {
  role: string;
  stopReason?: string;
  errorMessage?: string;
  content?: { type: string; text?: string }[];
}
interface PiEvent {
  reason?: string;
  text?: string;
  source?: string;
  streamingBehavior?: string;
  toolName?: string;
  kind?: string;
  title?: string;
  aborted?: boolean;
  messages?: Message[];
}
interface Pi {
  on(event: string, handler: (event: PiEvent, ctx: Context) => void): void;
}

export default function (pi: Pi) {
  const report = (event: string, ctx: Context, fields: Record<string, unknown> = {}, sync = false) => {
    const body = JSON.stringify({
      session_id: ctx.sessionManager.getSessionId(),
      transcript_path: ctx.sessionManager.getSessionFile() ?? null,
      cwd: ctx.cwd,
      // Pi runs as node: the hook takes the agent's process from here
      pid: process.pid,
      ...fields,
    });
    if (sync) {
      // Pi exits right after session_shutdown, before an async child reads
      try {
        spawnSync(cli, ["hook", "pi", event], { input: body, stdio: ["pipe", "ignore", "ignore"], timeout: 1500 });
      } catch {
        // a report Pinrail misses must never stop Pi
      }
      return;
    }
    try {
      const child = spawn(cli, ["hook", "pi", event], { stdio: ["pipe", "ignore", "ignore"] });
      child.on("error", () => {});
      child.stdin.on("error", () => {});
      child.stdin.end(body);
    } catch {
      // a report Pinrail misses must never stop Pi
    }
  };
  // the turn's reply and how it ended, from its last assistant message
  let last: Record<string, unknown> = {};
  pi.on("session_start", (e, ctx) => report("session-start", ctx, { reason: e.reason }));
  pi.on("input", (e, ctx) => {
    if (e.source !== "extension") report("prompt", ctx, { prompt: e.text, streaming: e.streamingBehavior ?? null });
  });
  pi.on("tool_execution_start", (e, ctx) => report("tool", ctx, { tool_name: e.toolName }));
  pi.on("ui_prompt_start", (e, ctx) => report("waiting", ctx, { kind: e.kind, title: e.title }));
  pi.on("ui_prompt_end", (e, ctx) => report("answered", ctx, { kind: e.kind }));
  pi.on("agent_end", (e) => {
    const message = [...(e.messages ?? [])].reverse().find((m) => m.role === "assistant");
    last = {
      stop_reason: message?.stopReason ?? null,
      error: message?.errorMessage ?? null,
      reply: (message?.content ?? [])
        .filter((c) => c.type === "text")
        .map((c) => c.text ?? "")
        .join(""),
    };
  });
  pi.on("agent_settled", (e, ctx) => {
    report("stop", ctx, { aborted: e.aborted, ...last });
    last = {};
  });
  pi.on("session_shutdown", (e, ctx) => report("session-end", ctx, { reason: e.reason }, true));
}
