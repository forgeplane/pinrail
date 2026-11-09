import { describe, expect, test } from "vitest";
import { composing, isTyping } from "./keys";

const press = (init: Partial<KeyboardEvent>) =>
  ({ key: "Enter", isComposing: false, keyCode: 13, ...init }) as KeyboardEvent;

describe("composing", () => {
  test("is a press an input method takes, as Chrome or WebKit marks it", () => {
    expect(composing(press({ isComposing: true }))).toBe(true);
    expect(composing(press({ keyCode: 229 }))).toBe(true);
    expect(composing({ nativeEvent: press({ isComposing: true }) })).toBe(true);
  });
  test("is not a plain press", () => {
    expect(composing(press({}))).toBe(false);
    expect(composing({ nativeEvent: press({}) })).toBe(false);
  });
});

describe("isTyping", () => {
  const el = (tagName: string, isContentEditable = false) => ({ tagName, isContentEditable }) as unknown as EventTarget;
  test("is a key press in a field that takes text or a choice", () => {
    for (const tag of ["INPUT", "TEXTAREA", "SELECT"]) expect(isTyping(el(tag))).toBe(true);
    expect(isTyping(el("DIV", true))).toBe(true);
  });
  test("is not one anywhere else", () => {
    expect(isTyping(el("DIV"))).toBe(false);
    expect(isTyping(el("BUTTON"))).toBe(false);
    expect(isTyping(null)).toBe(false);
  });
});
