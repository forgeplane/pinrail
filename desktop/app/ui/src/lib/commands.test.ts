import { describe, expect, test } from "vitest";
import { COMMANDS } from "./commands";
import { isMac } from "./keys";
import { SHORTCUTS } from "./shortcuts";

describe("the commands", () => {
  test("list every key they answer in the shortcuts the ? dialog shows", () => {
    const shown = SHORTCUTS.flatMap((s) => s.keys.map((combo) => combo.join(" ")));
    for (const command of COMMANDS)
      for (const combo of command.keys ?? [])
        expect(shown, `${command.id}: ${combo.join(" ")}`).toContain(combo.join(" "));
  });

  test("have an id each, and a key check for every key they list but ?", () => {
    const ids = COMMANDS.map((c) => c.id);
    expect(new Set(ids).size).toBe(ids.length);
    for (const command of COMMANDS)
      if (command.keys && command.id !== "shortcuts") expect(command.matches, command.id).toBeDefined();
  });

  test("answer the key presses they list", () => {
    const press = (key: string, mods: Partial<KeyboardEvent> = {}) =>
      ({ key, metaKey: false, ctrlKey: false, altKey: false, shiftKey: false, ...mods }) as KeyboardEvent;
    const by = (key: string, mods: Partial<KeyboardEvent> = {}) =>
      COMMANDS.filter((c) => c.matches?.(press(key, mods))).map((c) => c.id);
    // ⌘ on macOS, Ctrl elsewhere
    const mod = isMac ? { metaKey: true } : { ctrlKey: true };
    expect(by("3", mod)).toEqual(["go-items"]);
    expect(by("I", mod)).toEqual(["go-inbox"]);
    expect(by("C", { ...mod, shiftKey: true })).toEqual(["add-item"]);
    expect(by("ArrowDown", { altKey: true })).toEqual(["next-waiting"]);
    expect(by("c", mod)).toEqual([]);
    expect(by("3")).toEqual([]);
  });
});
