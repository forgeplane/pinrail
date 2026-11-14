import { describe, expect, test } from "vitest";
import { isViewToggle, reachesApp } from "./terminalKeys";

const key = (code: string, mods: Partial<Record<"ctrlKey" | "metaKey" | "shiftKey" | "altKey", boolean>> = {}) => ({
  key: code,
  code,
  ctrlKey: false,
  metaKey: false,
  shiftKey: false,
  altKey: false,
  ...mods,
});

describe("keys in a terminal on Linux", () => {
  test("Ctrl combinations go to the program", () => {
    for (const code of ["KeyB", "KeyK", "KeyR", "KeyC", "KeyV"])
      expect(reachesApp(key(code, { ctrlKey: true }), false)).toBe(false);
  });
  test("Ctrl+Shift combinations and Ctrl+` reach the app", () => {
    expect(reachesApp(key("KeyK", { ctrlKey: true, shiftKey: true }), false)).toBe(true);
    expect(reachesApp(key("KeyF", { ctrlKey: true, shiftKey: true }), false)).toBe(true);
    expect(reachesApp(key("Backquote", { ctrlKey: true }), false)).toBe(true);
  });
  test("plain keys and Escape go to the program", () => {
    for (const code of ["KeyJ", "Escape", "Digit1", "ArrowUp"]) expect(reachesApp(key(code), false)).toBe(false);
    expect(reachesApp(key("KeyE", { altKey: true }), false)).toBe(false);
  });
});

describe("keys in a terminal on macOS", () => {
  test("Cmd combinations reach the app, and Ctrl ones go to the program", () => {
    expect(reachesApp(key("KeyK", { metaKey: true }), true)).toBe(true);
    expect(reachesApp(key("BracketLeft", { metaKey: true }), true)).toBe(true);
    expect(reachesApp(key("KeyR", { ctrlKey: true }), true)).toBe(false);
    expect(reachesApp(key("Escape"), true)).toBe(false);
  });
  test("Ctrl+` reaches the app", () => {
    expect(reachesApp(key("Backquote", { ctrlKey: true }), true)).toBe(true);
  });
});

describe("the view toggle", () => {
  test("is Ctrl+` and MOD+Shift+T", () => {
    expect(isViewToggle(key("Backquote", { ctrlKey: true }), true)).toBe(true);
    expect(isViewToggle(key("KeyT", { metaKey: true, shiftKey: true }), true)).toBe(true);
    expect(isViewToggle(key("KeyT", { ctrlKey: true, shiftKey: true }), false)).toBe(true);
    expect(isViewToggle(key("KeyT", { ctrlKey: true, shiftKey: true }), true)).toBe(false);
    expect(isViewToggle(key("KeyT", { metaKey: true }), true)).toBe(false);
  });
});
