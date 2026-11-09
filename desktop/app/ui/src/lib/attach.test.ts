import { describe, expect, test } from "vitest";
import { fileName, freeName, linkedIn, markdownFor } from "./attach";

describe("fileName", () => {
  test("turns spaces, brackets and slashes into dashes", () => {
    expect(fileName("latency chart (final).png")).toBe("latency-chart-final-.png");
    expect(fileName("a/b\\c[1]<2>.txt")).toBe("a-b-c-1-2-.txt");
  });
  test("is not hidden, and drops control characters", () => {
    expect(fileName(".env")).toBe("env");
    expect(fileName("..-notes.md")).toBe("notes.md");
    expect(fileName("bad\u0007name.txt")).toBe("badname.txt");
  });
  test("is at most 120 characters, its extension kept", () => {
    const name = fileName(`${"a".repeat(200)}.png`);
    expect(name).toHaveLength(120);
    expect(name.endsWith(".png")).toBe(true);
  });
  test("keeps a name within 120 characters when its extension is long", () => {
    expect(fileName(`a.${"x".repeat(200)}`).length).toBeLessThanOrEqual(120);
  });
  test("is file when nothing is left", () => {
    expect(fileName("...")).toBe("file");
  });
});

describe("freeName", () => {
  test("is the name, or the first free name-2, name-3…", () => {
    const taken = new Set(["a.png", "a-2.png"]);
    expect(freeName("b.png", (n) => taken.has(n))).toBe("b.png");
    expect(freeName("a.png", (n) => taken.has(n))).toBe("a-3.png");
    expect(freeName("notes", (n) => n === "notes")).toBe("notes-2");
  });
  test("stays within 120 characters", () => {
    const long = fileName(`${"a".repeat(200)}.png`);
    const next = freeName(long, (n) => n === long);
    expect(next.length).toBeLessThanOrEqual(120);
    expect(next.endsWith("-2.png")).toBe(true);
  });
});

test("markdownFor shows an image in place and links any other file", () => {
  expect(markdownFor("a.png", "image/png")).toBe("![a.png](attachment:a.png)");
  expect(markdownFor("run.log", "text/plain")).toBe("[run.log](attachment:run.log)");
});

test("linkedIn keeps only the files the text still links to", () => {
  const file = { sha256: "x", size: 1, media_type: "text/plain" };
  const files = { "a.png": file, "b.log": file };
  expect(linkedIn(files, "See ![a.png](attachment:a.png).")).toEqual({ "a.png": file });
  expect(linkedIn(files, "nothing")).toEqual({});
});
