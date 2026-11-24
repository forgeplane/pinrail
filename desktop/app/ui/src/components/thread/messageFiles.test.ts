import { expect, test } from "vitest";
import { quotePath } from "../../lib/shell";
import { withPaths, type MessageFile } from "./messageFiles";

const file = (path: string | null): MessageFile => ({ key: path ?? "k", name: "f", type: "", path });

test("a path is quoted only when a shell would split or expand it", () => {
  expect(quotePath("/data/sessions/a/b/files/shot.png")).toBe("/data/sessions/a/b/files/shot.png");
  expect(quotePath("/Users/me/Application Support/shot.png")).toBe("'/Users/me/Application Support/shot.png'");
  expect(quotePath("/tmp/it's.png")).toBe("'/tmp/it'\\''s.png'");
});

test("the paths follow the text, one per line, leaving out a file not saved yet", () => {
  expect(withPaths(" Look ", [file("/a.png"), file(null), file("/b.md")])).toBe("Look\n\n/a.png\n/b.md");
  expect(withPaths("", [file("/a.png")])).toBe("/a.png");
  expect(withPaths("Just text", [])).toBe("Just text");
});
