import { describe, expect, test } from "vitest";
import { failedRenderers, loadRenderer, rendererOrder, type Registry, type TerminalRenderer } from "./view";

/** A renderer whose load succeeds or fails, and that counts its loads. */
function renderer(id: string, works = true) {
  const made = { loads: 0 };
  const make = async (): Promise<TerminalRenderer> => ({
    id,
    load: async () => {
      made.loads++;
      if (!works) throw new Error(`${id} cannot run here`);
    },
    create: () => {
      throw new Error("not used");
    },
  });
  return { make, made };
}

describe("rendererOrder", () => {
  test("auto stands for the chosen renderer, then xterm.js", () => {
    expect(rendererOrder(undefined)).toEqual(["ghostty", "xterm"]);
    expect(rendererOrder([])).toEqual(["ghostty", "xterm"]);
    expect(rendererOrder(["auto"])).toEqual(["ghostty", "xterm"]);
  });
  test("the setting's order is kept, with auto expanded in place", () => {
    expect(rendererOrder(["xterm"])).toEqual(["xterm"]);
    expect(rendererOrder(["xterm", "auto"])).toEqual(["xterm", "ghostty"]);
  });
  test("the development flag is tried first", () => {
    expect(rendererOrder(["auto"], "xterm")).toEqual(["xterm", "ghostty"]);
  });
});

describe("loadRenderer", () => {
  test("falls back to the next renderer, and does not try a failed one again", async () => {
    const broken = renderer("broken", false);
    const working = renderer("working");
    const registry: Registry = { broken: broken.make, working: working.make };
    expect((await loadRenderer(["broken", "working"], registry)).id).toBe("working");
    expect(failedRenderers().get("broken")).toBe("broken cannot run here");
    expect((await loadRenderer(["broken", "working"], registry)).id).toBe("working");
    expect(broken.made.loads).toBe(1);
  });
  test("skips what is not registered, and says why when nothing loads", async () => {
    await expect(loadRenderer(["missing"], {})).rejects.toThrow("missing: not registered");
  });
});
