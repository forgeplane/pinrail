// The keyboard of a libghostty terminal: a hidden textarea has the focus,
// at the cursor's cell so that an input method's candidates open there.
// Keys go to libghostty's key encoder, for the program's modes; the
// clipboard keys and Shift+Enter are handled before it, and the window's own
// shortcuts are left to it. A composition, from an input method or a dead
// key, sends nothing until it ends, and then its text as one input. Paste
// goes through libghostty's paste encoder. Focus reports go to a program
// that asked for them.

import type { InputKind } from "../../view";

/** Which Option keys act as Meta on macOS. */
export type OptionAsMeta = "off" | "left" | "right" | "both";

/** What the keyboard needs of the terminal. */
export interface KeyTarget {
  encodeKey(event: KeyboardEvent): Uint8Array;
  encodePaste(text: string): Uint8Array;
  mode(value: number): boolean;
  selectionText(): string | null;
}

export interface KeyboardOptions {
  mac: boolean;
  /** Ctrl+C and Ctrl+V copy and paste as well, as in Windows Terminal. */
  windows?: boolean;
  optionAsMeta: OptionAsMeta;
  /** The keys Shift+Enter sends, a new line in the agent's input, or null
   *  to encode Shift+Enter as any other key. */
  newline: Uint8Array | null;
  /** Copies text the person selected. */
  copy(text: string): void;
}

/** The character a key gives without Option, by its code, on a US layout,
 *  plain and with Shift; WebKit has no API for the layout in use. */
const US: Record<string, [string, string]> = {
  Backquote: ["`", "~"],
  Minus: ["-", "_"],
  Equal: ["=", "+"],
  BracketLeft: ["[", "{"],
  BracketRight: ["]", "}"],
  Backslash: ["\\", "|"],
  Semicolon: [";", ":"],
  Quote: ["'", '"'],
  Comma: [",", "<"],
  Period: [".", ">"],
  Slash: ["/", "?"],
  Space: [" ", " "],
  Digit1: ["1", "!"],
  Digit2: ["2", "@"],
  Digit3: ["3", "#"],
  Digit4: ["4", "$"],
  Digit5: ["5", "%"],
  Digit6: ["6", "^"],
  Digit7: ["7", "&"],
  Digit8: ["8", "*"],
  Digit9: ["9", "("],
  Digit0: ["0", ")"],
};

/** The character of a key without Option, or null when it has none. */
export function unmodifiedKey(code: string, shift: boolean): string | null {
  const letter = /^Key([A-Z])$/.exec(code)?.[1];
  if (letter) return shift ? letter : letter.toLowerCase();
  const pair = US[code];
  return pair ? pair[shift ? 1 : 0] : null;
}

/** Whether Option acts as Meta for a key on macOS, given which Option is
 *  held. */
export function optionIsMeta(setting: OptionAsMeta, rightOption: boolean): boolean {
  return setting === "both" || setting === (rightOption ? "right" : "left");
}

/**
 * The key to encode for a key pressed with Option on macOS. As Meta, the
 * key without Option and with Alt, which the encoder sends as ESC and the
 * key; otherwise the character Option composes, without Alt, sent alone.
 * A named key, such as an arrow, Backspace or Enter, composes nothing and
 * keeps Alt, so that Option+Left moves by a word.
 */
export function optionKey(event: KeyboardEvent, setting: OptionAsMeta, rightOption: boolean): KeyboardEvent {
  const meta = optionIsMeta(setting, rightOption);
  const key = meta ? (unmodifiedKey(event.code, event.shiftKey) ?? event.key) : event.key;
  const named = event.key.length > 1 && event.key !== "Dead" && event.key !== "Unidentified";
  // the encoder reads these, and getModifierState, from the event
  return {
    key,
    code: event.code,
    repeat: event.repeat,
    isComposing: event.isComposing,
    shiftKey: event.shiftKey,
    ctrlKey: event.ctrlKey,
    metaKey: event.metaKey,
    altKey: meta || named,
    getModifierState: (k: string) => event.getModifierState?.(k) ?? false,
  } as KeyboardEvent;
}

/** DEC private mode 1004: the program wants focus reports. */
const FOCUS_REPORTS = 1004;

export class Keyboard {
  readonly input: HTMLTextAreaElement;
  private composing = false;
  /** The keydown just handled, so its input event sends nothing more. */
  private handled = false;
  private rightOption = false;
  private cleanup: (() => void)[] = [];
  /** Where the input was last placed, in CSS pixels. */
  private at = { left: NaN, top: NaN, height: NaN };

  constructor(
    host: HTMLElement,
    private readonly target: KeyTarget,
    private readonly emit: (bytes: Uint8Array, kind: InputKind) => void,
    private options: KeyboardOptions,
  ) {
    const input = document.createElement("textarea");
    input.className = "ghostty-input";
    input.setAttribute("aria-label", "Terminal input");
    input.autocapitalize = "off";
    input.spellcheck = false;
    input.setAttribute("autocomplete", "off");
    input.setAttribute("autocorrect", "off");
    host.append(input);
    this.input = input;
    this.on("keydown", (e) => this.keyDown(e as KeyboardEvent));
    this.on("keyup", (e) => {
      if ((e as KeyboardEvent).key === "Alt") this.rightOption = false;
    });
    this.on("compositionstart", () => {
      this.composing = true;
      input.classList.add("is-composing");
    });
    this.on("compositionend", (e) => {
      this.composing = false;
      input.classList.remove("is-composing");
      const text = (e as CompositionEvent).data;
      if (text) this.emit(new TextEncoder().encode(text), "key");
      input.value = "";
    });
    // text a keydown did not send, such as after a dead key
    this.on("input", (e) => {
      const event = e as InputEvent;
      if (!this.composing && !this.handled && event.inputType === "insertText" && event.data) {
        this.emit(new TextEncoder().encode(event.data), "key");
      }
      if (!this.composing) input.value = "";
      this.handled = false;
    });
    // the Edit menu's Copy, and Cmd+C when the keydown left it to the menu
    this.on("copy", (e) => {
      const event = e as ClipboardEvent;
      const text = this.target.selectionText();
      if (!text || !event.clipboardData) return;
      event.clipboardData.setData("text/plain", text);
      event.preventDefault();
    });
    this.on("paste", (e) => {
      const event = e as ClipboardEvent;
      const text = event.clipboardData?.getData("text/plain") ?? "";
      event.preventDefault();
      if (text) this.emit(this.target.encodePaste(text), "paste");
    });
  }

  /** Pastes text as the person's paste, through the paste encoder. */
  paste(text: string) {
    if (text) this.emit(this.target.encodePaste(text), "paste");
  }

  /** Sends a key press as the person's, through the key encoder. */
  press(init: KeyboardEventInit) {
    const encoded = this.target.encodeKey(new KeyboardEvent("keydown", init));
    if (encoded.length) this.emit(encoded, "key");
  }

  setOptions(options: Partial<KeyboardOptions>) {
    this.options = { ...this.options, ...options };
  }

  private on(type: string, listener: (event: Event) => void) {
    this.input.addEventListener(type, listener);
    this.cleanup.push(() => this.input.removeEventListener(type, listener));
  }

  private keyDown(event: KeyboardEvent) {
    this.handled = false;
    if (event.key === "Alt") this.rightOption = event.code === "AltRight";
    // a composition sends its text when it ends
    if (event.isComposing || this.composing || event.keyCode === 229) return;
    const { mac, windows } = this.options;
    const key = event.key.toLowerCase();
    // copy: Cmd+C on macOS, Ctrl+Shift+C elsewhere, with a selection
    const clipboardKey = mac ? event.metaKey && !event.ctrlKey : event.ctrlKey && event.shiftKey;
    // and on Windows Ctrl+C, which is the program's interrupt when nothing
    // is selected, and Ctrl+V, as Windows Terminal takes them
    const windowsKey = !!windows && event.ctrlKey && !event.shiftKey && !event.altKey && !event.metaKey;
    if ((clipboardKey || windowsKey) && key === "c") {
      const text = this.target.selectionText();
      if (text) {
        event.preventDefault();
        this.options.copy(text);
        return;
      }
      if (clipboardKey) return;
    }
    // paste: the browser's paste event follows, which encodes it
    if ((clipboardKey || windowsKey) && key === "v") return;
    // the window's own shortcuts: Cmd on macOS, Ctrl+Shift on Linux
    if (mac ? event.metaKey : event.ctrlKey && event.shiftKey && !event.metaKey) return;
    if (event.key === "Enter" && event.shiftKey && !event.ctrlKey && !event.altKey && this.options.newline) {
      event.preventDefault();
      this.handled = true;
      this.emit(this.options.newline, "key");
      return;
    }
    const encoded = this.target.encodeKey(
      mac && event.altKey ? optionKey(event, this.options.optionAsMeta, this.rightOption) : event,
    );
    if (encoded.length === 0) return;
    event.preventDefault();
    this.handled = true;
    this.emit(encoded, "key");
  }

  /** Puts the textarea over the cursor's cell, where an input method's
   *  candidates and marked text then show. */
  moveTo(left: number, top: number, height: number) {
    if (left === this.at.left && top === this.at.top && height === this.at.height) return;
    this.at = { left, top, height };
    this.input.style.left = `${left}px`;
    this.input.style.top = `${top}px`;
    this.input.style.height = `${height}px`;
  }

  focus() {
    this.input.focus({ preventScroll: true });
    this.report(true);
  }

  blur() {
    this.input.blur();
    this.report(false);
  }

  /** A focus report, to a program that asked for them. */
  private report(focused: boolean) {
    if (this.target.mode(FOCUS_REPORTS)) this.emit(new TextEncoder().encode(focused ? "\x1b[I" : "\x1b[O"), "focus");
  }

  dispose() {
    this.cleanup.forEach((c) => c());
    this.input.remove();
  }
}
