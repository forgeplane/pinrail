import { describe, expect, test } from "vitest";
import { optionIsMeta, optionKey, unmodifiedKey } from "./keyboard";

/** Option+B on macOS, which composes ∫. */
const optionB = (shift = false) =>
  ({
    key: shift ? "ı" : "∫",
    code: "KeyB",
    repeat: false,
    isComposing: false,
    shiftKey: shift,
    ctrlKey: false,
    metaKey: false,
    altKey: true,
    getModifierState: () => false,
  }) as unknown as KeyboardEvent;

describe("Option as Meta", () => {
  test("applies to the Option keys the setting names", () => {
    expect(optionIsMeta("off", false)).toBe(false);
    expect(optionIsMeta("left", false)).toBe(true);
    expect(optionIsMeta("left", true)).toBe(false);
    expect(optionIsMeta("right", true)).toBe(true);
    expect(optionIsMeta("both", true)).toBe(true);
    expect(optionIsMeta("both", false)).toBe(true);
  });

  test("as Meta, sends the key without Option, with Alt for the ESC", () => {
    const key = optionKey(optionB(), "left", false);
    expect([key.key, key.altKey]).toEqual(["b", true]);
    const shifted = optionKey(optionB(true), "both", true);
    expect([shifted.key, shifted.altKey]).toEqual(["B", true]);
  });

  test("otherwise, sends the character Option composes, alone", () => {
    const key = optionKey(optionB(), "off", false);
    expect([key.key, key.altKey]).toEqual(["∫", false]);
    const right = optionKey(optionB(), "left", true);
    expect([right.key, right.altKey]).toEqual(["∫", false]);
  });

  test("knows the key without Option for letters, digits and punctuation", () => {
    expect(unmodifiedKey("KeyQ", false)).toBe("q");
    expect(unmodifiedKey("Digit3", true)).toBe("#");
    expect(unmodifiedKey("BracketLeft", false)).toBe("[");
    expect(unmodifiedKey("F5", false)).toBeNull();
  });
});
