import { expect, test } from "vitest";
import { quotePath } from "../../lib/shell";
import { withPaths, type MessageFile } from "./messageFiles";

const file = (path: string | null): MessageFile => ({ key: path ?? "k", name: "f", type: "", path });

test("a path is quoted only when a shell would split or expand it", () => {
  expect(quotePath("/data/sessions/a/b/files/shot.png", false)).toBe("/data/sessions/a/b/files/shot.png");
  expect(quotePath("/Users/me/Application Support/shot.png", false)).toBe("'/Users/me/Application Support/shot.png'");
  expect(quotePath("/tmp/it's.png", false)).toBe("'/tmp/it'\\''s.png'");
});

test("a Windows path is as Windows Terminal drops one: in double quotes only when it must be", () => {
  expect(quotePath(String.raw`C:\Users\me\shot.png`, true)).toBe(String.raw`C:\Users\me\shot.png`);
  expect(quotePath(String.raw`C:\Users\me\My Docs\it's.png`, true)).toBe(String.raw`"C:\Users\me\My Docs\it's.png"`);
  expect(quotePath(String.raw`C:\a&b\x.png`, true)).toBe(String.raw`"C:\a&b\x.png"`);
  // what PowerShell or cmd would read otherwise: a variable, a delayed
  // expansion, a wildcard, a block, a comment, a splat
  for (const name of ["$final", "%TEMP%", "wow!", "[1]", "{a}", "#1", "@a"]) {
    const path = String.raw`C:\Users\me\report` + name + ".txt";
    expect(quotePath(path, true)).toBe(`"${path}"`);
  }
  // a short name, as a temporary folder can come in
  expect(quotePath(String.raw`C:\Users\ALICE~1\x.png`, true)).toBe(String.raw`C:\Users\ALICE~1\x.png`);
});

test("the paths follow the text, one per line, leaving out a file not saved yet", () => {
  expect(withPaths(" Look ", [file("/a.png"), file(null), file("/b.md")])).toBe("Look\n\n/a.png\n/b.md");
  expect(withPaths("", [file("/a.png")])).toBe("/a.png");
  expect(withPaths("Just text", [])).toBe("Just text");
});
