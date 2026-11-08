import { describe, expect, test } from "vitest";
import { composing } from "./keys";

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
