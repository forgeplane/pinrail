// Pinrail's plugin for OpenCode, installed by Pinrail when Report status is
// on for OpenCode, and removed when it is turned off: Pinrail rewrites this
// file, so add your own plugins beside it instead of editing it. It runs in
// OpenCode's terminal interface and reports the state of the session on
// screen to Pinrail by running `pinrail hook opencode <event>` with JSON on
// its input, as the hooks of other agents do.
import { spawn } from "node:child_process";
import process from "node:process";
import { clearInterval, setInterval } from "node:timers";

const cli = process.env.PINRAIL_CLI || "pinrail";

/** Runs `pinrail hook opencode <event>` with `body`; a report Pinrail misses
 *  must never stop OpenCode. */
function report(event, body, detached = false) {
  try {
    const child = spawn(cli, ["hook", "opencode", event], { stdio: ["pipe", "ignore", "ignore"], detached });
    child.on("error", () => {});
    child.stdin.on("error", () => {});
    child.stdin.end(JSON.stringify(body));
    if (detached) child.unref();
  } catch {
    // reported nothing
  }
}

export default {
  id: "pinrail.opencode",
  setup(api) {
    const cwd = process.cwd();
    const send = (event, session, fields = {}, detached = false) =>
      report(event, { session_id: session, cwd, pid: process.pid, ...fields }, detached);
    // the session on screen, which the reports are of
    const routed = () => {
      try {
        const route = api.ui.router.current();
        return route?.type === "session" ? route.sessionID : null;
      } catch {
        return null;
      }
    };
    // the sessions this window created, which start new rather than resume
    const created = new Set();
    // whether each session works on a message, the last text it wrote, and
    // the command of each tool call, which a permission it asks names
    const working = new Set();
    const replies = new Map();
    const calls = new Map();
    let shown = null;
    // the session on screen, which changes with /new, with a session opened
    // from the list, and when a first prompt creates one
    const follow = () => {
      const now = routed();
      if (now && now !== shown) {
        shown = now;
        send("session-start", now, { reason: created.has(now) ? "new" : "resume" });
      }
    };
    follow();
    const following = setInterval(follow, 500);
    const off = api.data.listen(({ details }) => {
      const data = details?.data ?? {};
      if (details?.type === "session.created" && !data.parentID) created.add(data.sessionID);
      // an event can come before the poll sees the session it is of
      follow();
      const session = data.sessionID ?? data.form?.sessionID;
      if (!session || session !== shown) return;
      switch (details.type) {
        case "session.inbox.enqueued":
          if (data.item?.type === "user")
            send("prompt", session, { prompt: data.item.payload?.text ?? null, working: working.has(session) });
          break;
        case "session.execution.started":
          working.add(session);
          break;
        case "session.tool.called":
          calls.set(data.id, data.input?.command ?? data.input?.description ?? null);
          send("tool", session);
          break;
        case "permission.asked":
          send("waiting", session, {
            kind: "permission",
            summary: calls.get(data.source?.id) || (data.resources ?? []).join(" ") || data.action || null,
          });
          break;
        case "form.created":
          if (data.form?.metadata?.kind === "question") {
            const field = data.form.fields?.[0];
            send("waiting", session, { kind: "question", summary: field?.description || field?.title || null });
          }
          break;
        case "permission.replied":
        case "form.replied":
          send("answered", session);
          break;
        case "session.text.ended":
          replies.set(session, data.text ?? null);
          break;
        case "session.execution.succeeded":
          working.delete(session);
          send("stop", session, { end: "done", reply: replies.get(session) ?? null });
          replies.delete(session);
          break;
        case "session.execution.failed":
          working.delete(session);
          send("stop", session, { end: "failed", error: data.error?.message ?? null });
          break;
        case "session.execution.interrupted":
          working.delete(session);
          send("stop", session, { end: "interrupted" });
          break;
      }
    });
    let ended = false;
    return () => {
      clearInterval(following);
      off?.();
      // OpenCode runs this twice as it exits, and leaves before a child
      // that is not detached reads its input
      if (ended || !shown) return;
      ended = true;
      send("session-end", shown, {}, true);
    };
  },
};
