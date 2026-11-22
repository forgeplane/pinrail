import { describe, expect, test } from "vitest";
import type { ItemDetail, ItemEntry, ItemFile } from "../api/types";
import { itemTask } from "./itemTask";

const file = (name: string): ItemFile => ({ name, sha256: "0".repeat(64), size: 10, media_type: "image/png" });

const comment = (author: string, body: string, files: ItemFile[] = []): ItemEntry => ({
  type: "comment",
  id: `c-${author}`,
  body,
  author,
  session: null,
  created_at: "2026-11-20T09:30:00Z",
  edited_at: null,
  files,
});

const event: ItemEntry = {
  type: "event",
  id: 1,
  kind: "claimed",
  actor: "claude-code",
  session: null,
  at: "2026-11-20T09:00:00Z",
  attrs: null,
};

const item = (over: Partial<ItemDetail> = {}) =>
  ({ key: "ACME-7", title: "Fix the login", body: "", files: [], timeline: [], ...over }) as Pick<
    ItemDetail,
    "key" | "title" | "body" | "files" | "timeline"
  >;

describe("itemTask", () => {
  test("is the key and title as a heading, then the description", () => {
    expect(itemTask(item({ body: "The redirect loops after a sign-in." }))).toBe(
      "# ACME-7 Fix the login\n\nThe redirect loops after a sign-in.",
    );
  });

  test("names attached files instead of linking them, and gives the command that saves them", () => {
    const task = itemTask(
      item({
        body: "It loops:\n\n![loop.png](attachment:loop.png)\n\nSee [trace.log](attachment:trace.log).",
        files: [file("loop.png"), file("trace.log")],
      }),
    );
    expect(task).not.toContain("attachment:");
    expect(task).toContain("It loops:\n\nattached file `loop.png`\n\nSee attached file `trace.log`.");
    expect(task).toContain(
      "## Attached files\n\n`loop.png`, `trace.log`. Run `pinrail items show ACME-7` to save them and print their paths.",
    );
  });

  test("includes the comments with their authors, and their files, but not other events", () => {
    const task = itemTask(
      item({
        body: "The redirect loops.",
        timeline: [
          event,
          comment("person", "Also on Safari:\n\n![safari.png](attachment:safari.png)", [file("safari.png")]),
          comment("claude-code", "The cookie is not set."),
        ],
      }),
    );
    expect(task).toBe(
      [
        "# ACME-7 Fix the login",
        "The redirect loops.",
        "## Comments",
        "**person**, 2026-11-20:\n\nAlso on Safari:\n\nattached file `safari.png`",
        "**claude-code**, 2026-11-20:\n\nThe cookie is not set.",
        "## Attached files",
        "`safari.png`. Run `pinrail items show ACME-7` to save it and print its path.",
      ].join("\n\n"),
    );
  });
});
