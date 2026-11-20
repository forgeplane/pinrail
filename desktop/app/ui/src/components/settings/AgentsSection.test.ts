import { describe, expect, test } from "vitest";
import { integrationNote, type Integration } from "./AgentsSection";

const hooks = (state: Integration["state"]): Integration => ({
  kind: "hooks",
  file: "/Users/you/.copilot/hooks/pinrail.json",
  state,
});
const extension = (state: Integration["state"]): Integration => ({
  kind: "extension",
  file: "/Users/you/.pi/agent/extensions/pinrail.ts",
  state,
});

describe("Report status", () => {
  test("says what it installs for an agent, and where", () => {
    expect(integrationNote(hooks("absent"), "Copilot CLI")).toEqual({
      note: "Installs Pinrail's hooks in ~/.copilot/hooks/pinrail.json so Pinrail can show when Copilot CLI works, waits for you or finishes.",
      update: false,
    });
    expect(integrationNote(extension("current"), "Pi").note).toBe(
      "Installs Pinrail's extension in ~/.pi/agent/extensions/pinrail.ts so Pinrail can show when Pi works, waits for you or finishes.",
    );
  });

  test("says that it adds a plugin to the agent's plugin list", () => {
    const plugin = (state: Integration["state"]): Integration => ({
      kind: "plugin",
      file: "/Users/you/.config/opencode/cli.json",
      state,
    });
    expect(integrationNote(plugin("absent"), "OpenCode").note).toBe(
      "Adds Pinrail's plugin to ~/.config/opencode/cli.json so Pinrail can show when OpenCode works, waits for you or finishes.",
    );
    expect(integrationNote(plugin("outdated"), "OpenCode")).toEqual({
      note: "Pinrail's plugin was installed by another version of Pinrail.",
      update: true,
    });
  });

  test("says what is wrong with the file, in the words of what it installs", () => {
    expect(integrationNote(hooks("outdated"), "Copilot CLI")).toEqual({
      note: "The hooks were installed by another version of Pinrail.",
      update: true,
    });
    expect(integrationNote(extension("outdated"), "Pi")).toEqual({
      note: "Pinrail's extension was installed by another version of Pinrail.",
      update: true,
    });
    expect(integrationNote(hooks("unreadable"), "Copilot CLI").note).toBe(
      "Pinrail cannot read ~/.copilot/hooks/pinrail.json, so it leaves the file as it is. Fix the file to turn this on.",
    );
    // a file of the person's where the extension goes
    expect(integrationNote(extension("unreadable"), "Pi")).toEqual({
      note: "~/.pi/agent/extensions/pinrail.ts is not Pinrail's extension, so Pinrail leaves it as it is. Move it to turn this on.",
      update: false,
    });
  });
});
