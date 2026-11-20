// Pinrail's extension for Pi, installed by Pinrail when Report status is on
// for Pi, and removed when it is turned off: Pinrail rewrites this file, so
// add your own extensions beside it instead of editing it. It reports the
// session's state to Pinrail by running `pinrail hook pi <event>` with JSON
// on its input, as the hooks of other agents do.
import { spawn, spawnSync } from "node:child_process";

const cli = process.env.PINRAIL_CLI || "pinrail";

export default function (pi: any) {
  const report = (event: string, ctx: any, fields: Record<string, unknown> = {}, sync = false) => {
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
      } catch {}
      return;
    }
    try {
      const child = spawn(cli, ["hook", "pi", event], { stdio: ["pipe", "ignore", "ignore"] });
      child.on("error", () => {});
      child.stdin.on("error", () => {});
      child.stdin.end(body);
    } catch {}
  };
  // the turn's reply and how it ended, from its last assistant message
  let last: Record<string, unknown> = {};
  pi.on("session_start", (e: any, ctx: any) => report("session-start", ctx, { reason: e.reason }));
  pi.on("input", (e: any, ctx: any) => {
    if (e.source !== "extension") report("prompt", ctx, { prompt: e.text, streaming: e.streamingBehavior ?? null });
  });
  pi.on("tool_execution_start", (e: any, ctx: any) => report("tool", ctx, { tool_name: e.toolName }));
  pi.on("ui_prompt_start", (e: any, ctx: any) => report("waiting", ctx, { kind: e.kind, title: e.title }));
  pi.on("ui_prompt_end", (e: any, ctx: any) => report("answered", ctx, { kind: e.kind }));
  pi.on("agent_end", (e: any) => {
    const message = [...(e.messages ?? [])].reverse().find((m: any) => m.role === "assistant");
    last = {
      stop_reason: message?.stopReason ?? null,
      error: message?.errorMessage ?? null,
      reply: (message?.content ?? [])
        .filter((c: any) => c.type === "text")
        .map((c: any) => c.text)
        .join(""),
    };
  });
  pi.on("agent_settled", (e: any, ctx: any) => {
    report("stop", ctx, { aborted: e.aborted, ...last });
    last = {};
  });
  pi.on("session_shutdown", (e: any, ctx: any) => report("session-end", ctx, { reason: e.reason }, true));
}
