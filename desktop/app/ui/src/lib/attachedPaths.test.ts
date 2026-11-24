import { expect, test } from "vitest";
import { attachedParts, attachedPath, isImage, withoutAttachments } from "./attachedPaths";

test("a line names an attached file only when it is that path alone, quoted or not", () => {
  expect(attachedPath("/data/sessions/claude-code/ses_1/files/shot.png")).toBe(
    "/data/sessions/claude-code/ses_1/files/shot.png",
  );
  expect(attachedPath("'/My Data/sessions/codex/ses_2/files/it'\\''s.md'")).toBe(
    "/My Data/sessions/codex/ses_2/files/it's.md",
  );
  expect(attachedPath("see /data/sessions/claude-code/ses_1/files/shot.png")).toBeNull();
  expect(attachedPath("/data/sessions/claude-code/files/shot.png")).toBeNull();
});

test("the text loses the attached paths and the image placeholders", () => {
  expect(withoutAttachments("[Image #2]Look\n\n/d/sessions/a/s/files/x.png")).toBe("Look\n");
});

test("a text splits into its words, its attached files and its image placeholders", () => {
  const parts = attachedParts(
    "[Image #1]Look at these\n\n/d/sessions/claude-code/ses_1/files/shot.png\n'/d d/sessions/codex/ses_2/files/notes.md'",
  );
  expect(parts.text).toBe("Look at these");
  expect(parts.images).toBe(1);
  expect(parts.files.map((f) => [f.agent, f.session, f.name, isImage(f.name)])).toEqual([
    ["claude-code", "ses_1", "shot.png", true],
    ["codex", "ses_2", "notes.md", false],
  ]);
});
