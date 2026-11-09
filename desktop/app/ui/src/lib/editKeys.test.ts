import { describe, expect, test, vi } from "vitest";
import { editKeys } from "./editKeys";
import { isMac } from "./keys";

// a key press as React gives it
const press = (key: string, extra: Partial<KeyboardEvent> = {}) => {
  const native = { key, isComposing: false, keyCode: 0, ctrlKey: false, metaKey: false, ...extra } as KeyboardEvent;
  return {
    key,
    shiftKey: false,
    ...extra,
    nativeEvent: native,
    preventDefault: vi.fn(),
    stopPropagation: vi.fn(),
  } as unknown as Parameters<ReturnType<typeof editKeys>>[0];
};

describe("editKeys", () => {
  test("Enter saves a line, and Shift+Enter does not", () => {
    const onSave = vi.fn();
    const keys = editKeys({ onSave, onCancel: vi.fn() });
    keys(press("Enter", { shiftKey: true }));
    expect(onSave).not.toHaveBeenCalled();
    keys(press("Enter"));
    expect(onSave).toHaveBeenCalledOnce();
  });
  test("in a field of several lines, only the modifier and Enter save", () => {
    const onSave = vi.fn();
    const keys = editKeys({ onSave, onCancel: vi.fn(), saveOn: "mod-enter" });
    keys(press("Enter"));
    expect(onSave).not.toHaveBeenCalled();
    // ⌘ on a Mac, Ctrl elsewhere
    keys(press("Enter", isMac ? { metaKey: true } : { ctrlKey: true }));
    expect(onSave).toHaveBeenCalledOnce();
  });
  test("Esc cancels and goes no further", () => {
    const onCancel = vi.fn();
    const event = press("Escape");
    editKeys({ onSave: vi.fn(), onCancel })(event);
    expect(onCancel).toHaveBeenCalledOnce();
    expect(event.stopPropagation).toHaveBeenCalled();
  });
  test("a key an input method composes with is the field's", () => {
    const onSave = vi.fn();
    editKeys({ onSave, onCancel: vi.fn() })(press("Enter", { isComposing: true }));
    expect(onSave).not.toHaveBeenCalled();
  });
});
