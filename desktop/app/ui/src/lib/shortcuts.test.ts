import { afterEach, describe, expect, test, vi } from "vitest";

// the shortcuts module reads the platform when it loads: load it fresh as
// macOS or as Linux
async function on(platform: "MacIntel" | "Linux x86_64") {
  vi.resetModules();
  vi.stubGlobal("navigator", { platform });
  return import("./shortcuts");
}
afterEach(() => vi.unstubAllGlobals());

const press = (code: string, mods: Partial<Record<"ctrlKey" | "altKey" | "shiftKey" | "metaKey", boolean>> = {}) =>
  ({ code, ctrlKey: false, altKey: false, shiftKey: false, metaKey: false, ...mods }) as KeyboardEvent;

describe("comboFromEvent", () => {
  test("names the physical key, with its modifiers in a fixed order", async () => {
    const { comboFromEvent } = await on("MacIntel");
    expect(comboFromEvent(press("KeyJ"))).toBe("j");
    expect(comboFromEvent(press("Digit1"))).toBe("1");
    expect(comboFromEvent(press("Enter"))).toBe("enter");
    expect(comboFromEvent(press("ArrowUp"))).toBe("arrowup");
    expect(comboFromEvent(press("KeyH", { shiftKey: true, ctrlKey: true }))).toBe("ctrl+shift+h");
    expect(comboFromEvent(press("KeyK", { metaKey: true, shiftKey: true, altKey: true, ctrlKey: true }))).toBe(
      "ctrl+alt+shift+cmd+k",
    );
  });

  test("writes punctuation as the character it types without Shift", async () => {
    const { comboFromEvent } = await on("MacIntel");
    expect(comboFromEvent(press("Slash", { shiftKey: true }))).toBe("shift+/");
    expect(comboFromEvent(press("BracketLeft"))).toBe("[");
    expect(comboFromEvent(press("Comma", { metaKey: true }))).toBe("cmd+,");
  });

  test("is nothing for a modifier on its own", async () => {
    const { comboFromEvent } = await on("MacIntel");
    for (const code of ["ShiftLeft", "ControlRight", "AltLeft", "MetaRight", "OSLeft", ""]) {
      expect(comboFromEvent(press(code)), code).toBeNull();
    }
  });
});

describe("shortcutFromEvent", () => {
  test("needs a modifier, so it never takes plain typing", async () => {
    const { shortcutFromEvent } = await on("MacIntel");
    expect(shortcutFromEvent(press("KeyW"))).toBeNull();
    expect(shortcutFromEvent(press("KeyW", { shiftKey: true }))).toBeNull();
    expect(shortcutFromEvent(press("KeyW", { altKey: true, shiftKey: true }))).toBe("alt+shift+w");
    expect(shortcutFromEvent(press("AltLeft", { altKey: true }))).toBeNull();
  });
});

describe("shortcutGlyphs", () => {
  test("shows macOS glyphs on macOS and key names on Linux", async () => {
    const mac = await on("MacIntel");
    expect(mac.shortcutGlyphs("alt+shift+w")).toEqual(["⌥", "⇧", "W"]);
    expect(mac.shortcutGlyphs("cmdorctrl+k")).toEqual(["⌘", "K"]);
    const linux = await on("Linux x86_64");
    expect(linux.shortcutGlyphs("alt+shift+w")).toEqual(["Alt", "Shift", "W"]);
    expect(linux.shortcutGlyphs("cmdorctrl+k")).toEqual(["Ctrl", "K"]);
    expect(linux.shortcutGlyphs("cmd+k")).toEqual(["Win", "K"]);
  });

  test("shows named keys as the key they are", async () => {
    const { shortcutGlyphs } = await on("MacIntel");
    expect(shortcutGlyphs("arrowup")).toEqual(["↑"]);
    expect(shortcutGlyphs("keya")).toEqual(["A"]);
    expect(shortcutGlyphs("digit3")).toEqual(["3"]);
    expect(shortcutGlyphs("slash")).toEqual(["/"]);
    expect(shortcutGlyphs(" alt + enter ")).toEqual(["⌥", "↩"]);
  });
});

describe("the app's own keys", () => {
  test("are the platform's: ⌘ on macOS, Ctrl on Linux", async () => {
    const mac = await on("MacIntel");
    expect(mac.isShadowed("cmd+k")).toBe(true);
    expect(mac.isShadowed("ctrl+k")).toBe(false);
    // modifiers in the order the core writes them too: ctrl, alt, shift, cmd
    expect(mac.isShadowed("shift+cmd+h")).toBe(true);
    const linux = await on("Linux x86_64");
    expect(linux.isShadowed("ctrl+k")).toBe(true);
    expect(linux.isShadowed("cmd+k")).toBe(false);
  });

  test("include the review screen's keys everywhere, and leave a view the rest", async () => {
    for (const platform of ["MacIntel", "Linux x86_64"] as const) {
      const { isShadowed } = await on(platform);
      for (const keys of ["shift+/", "[", "]", "escape", "cmd+enter", "ctrl+enter"]) {
        expect(isShadowed(keys), `${platform}: ${keys}`).toBe(true);
      }
      expect(isShadowed("j"), platform).toBe(false);
      expect(isShadowed("alt+shift+w"), platform).toBe(false);
    }
  });
});

describe("the shortcut list", () => {
  test("on Linux shows no macOS glyphs", async () => {
    const { SHORTCUTS } = await on("Linux x86_64");
    const shown = SHORTCUTS.flatMap((s) => s.keys.flat()).join(" ");
    expect(shown).not.toMatch(/[⌘⌥⇧⌃]/);
    expect(shown).toContain("Ctrl");
  });
});
