// Pinrail's OpenCode plugin, with a stand-in for OpenCode's plugin API: it
// lets OpenCode read the files attached to Pinrail's sessions without
// asking, and leaves every other permission to the person.
import { afterEach, expect, test } from "vitest";

process.env.PINRAIL_CLI = "/nonexistent/pinrail";
process.env.PINRAIL_DATA_DIR = "/data/pinrail";
const { default: plugin } = await import("./opencode-plugin.js");

const stops = [];
afterEach(() => stops.splice(0).forEach((stop) => stop()));

/** The plugin set up against a stand-in API, on session `ses_1`, and what
 *  it answered. */
function opened() {
  const replies = [];
  let handler = () => {};
  const api = {
    ui: { router: { current: () => ({ type: "session", sessionID: "ses_1" }) } },
    data: {
      listen: (fn) => {
        handler = fn;
        return () => {};
      },
    },
    client: { permission: { reply: async (input) => replies.push(input) } },
  };
  stops.push(plugin.setup(api));
  const ask = (id, action, resources) =>
    handler({ details: { type: "permission.asked", data: { id, sessionID: "ses_1", action, resources } } });
  return { replies, ask, handle: (details) => handler({ details }) };
}

const folder = "/data/pinrail/sessions/opencode/ses_01/files";

test("reading the files attached to a session is allowed once, without asking", async () => {
  const { replies, ask } = opened();
  ask("per_1", "external_directory", [`${folder}/*`]);
  await Promise.resolve();
  expect(replies).toEqual([{ sessionID: "ses_1", requestID: "per_1", decision: "once" }]);
});

test("a subagent's session, not the one on screen, reads them without asking too", async () => {
  const { replies, handle } = opened();
  handle({
    type: "permission.asked",
    data: { id: "per_7", sessionID: "ses_sub", action: "external_directory", resources: [`${folder}/*`] },
  });
  await Promise.resolve();
  expect(replies).toEqual([{ sessionID: "ses_sub", requestID: "per_7", decision: "once" }]);
});

test("any other folder, a folder beside the sessions one, or another action still asks", async () => {
  const { replies, ask } = opened();
  ask("per_2", "external_directory", ["/Users/me/secrets/*"]);
  ask("per_3", "external_directory", [`${folder}/*`, "/etc/*"]);
  ask("per_4", "external_directory", ["/data/pinrail/sessions-old/x/*"]);
  ask("per_5", "external_directory", ["/data/pinrail/*"]);
  ask("per_6", "bash", [`${folder}/*`]);
  await Promise.resolve();
  expect(replies).toEqual([]);
});
