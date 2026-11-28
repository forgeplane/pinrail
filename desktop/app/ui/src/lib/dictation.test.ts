import { afterEach, describe, expect, test, vi } from "vitest";
import type { Heard } from "./dictation";

/** A stand-in for the web view's speech recognition, which the test drives. */
class FakeRecognition {
  static last: FakeRecognition | null = null;
  continuous = false;
  interimResults = false;
  lang = "";
  started = false;
  stopped = false;
  onresult: ((event: { results: ArrayLike<{ 0: { transcript: string }; isFinal: boolean }> }) => void) | null = null;
  onerror: ((event: { error: string }) => void) | null = null;
  onend: (() => void) | null = null;
  constructor() {
    FakeRecognition.last = this;
  }
  start() {
    this.started = true;
  }
  stop() {
    this.stopped = true;
  }
  abort() {}
  /** Recognizes these pieces, as the web view sends them, all so far. */
  hear(...pieces: string[]) {
    this.onresult?.({ results: pieces.map((transcript) => ({ 0: { transcript }, isFinal: false })) });
  }
}

/** The module, loaded in a window that has speech recognition. */
async function withRecognition() {
  vi.resetModules();
  vi.stubGlobal("window", { webkitSpeechRecognition: FakeRecognition });
  return import("./dictation");
}

afterEach(() => vi.unstubAllGlobals());

describe("dictationError", () => {
  test("says nothing when recognition was stopped or heard nothing", async () => {
    const { dictationError } = await import("./dictation");
    expect(dictationError("aborted")).toBeNull();
    expect(dictationError("no-speech")).toBeNull();
  });

  test("says what to do for a refused microphone, and names an unknown error", async () => {
    const { dictationError } = await import("./dictation");
    expect(dictationError("not-allowed", false)).toMatch(/System Settings › Privacy & Security/);
    expect(dictationError("service-not-allowed", false)).toBe(dictationError("not-allowed", false));
    // Windows has its own settings, which name it otherwise
    expect(dictationError("not-allowed", true)).toMatch(/Windows' Settings › Privacy & security › Microphone/);
    expect(dictationError("audio-capture")).toBe("No microphone was found.");
    expect(dictationError("bad-grammar")).toBe("Dictation stopped: bad-grammar.");
  });
});

describe("listen", () => {
  test("in a window without speech recognition, ends at once and says why", async () => {
    vi.resetModules();
    const { canDictate, listen } = await import("./dictation");
    const heard: Heard[] = [];
    listen((h) => heard.push(h));
    expect(canDictate).toBe(false);
    expect(heard).toEqual([{ done: true, error: "This window cannot turn speech into text." }]);
  });

  test("sends the text heard so far as it grows, then the end", async () => {
    const { canDictate, listen } = await withRecognition();
    expect(canDictate).toBe(true);
    const heard: Heard[] = [];
    const stop = listen((h) => heard.push(h));
    const recognition = FakeRecognition.last!;
    expect(recognition.started && recognition.continuous && recognition.interimResults).toBe(true);

    recognition.hear("Ship");
    recognition.hear("Ship", " the branch");
    stop();
    expect(recognition.stopped).toBe(true);
    recognition.onend?.();
    expect(heard).toEqual([{ text: "Ship" }, { text: "Ship the branch" }, { done: true }]);
  });

  test("an error ends it with its message; one that says nothing does not", async () => {
    const { listen } = await withRecognition();
    const heard: Heard[] = [];
    listen((h) => heard.push(h));
    const recognition = FakeRecognition.last!;
    recognition.onerror?.({ error: "audio-capture" });
    // a later error with no message keeps the first
    recognition.onerror?.({ error: "aborted" });
    recognition.onend?.();
    expect(heard).toEqual([{ done: true, error: "No microphone was found." }]);
  });
});
