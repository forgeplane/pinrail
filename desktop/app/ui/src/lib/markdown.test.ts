import { describe, expect, test } from "vitest";
import { diffRows } from "./diffView";
import { onItemPrefixes, renderLinks, renderMarkdown, renderNote, setItemPrefixes } from "./markdown";

describe("renderMarkdown", () => {
  test("highlights a block of code in the language it names", () => {
    const html = renderMarkdown("```rust\nfn main() { let n = 1; }\n```");
    expect(html).toContain('<span class="hljs-keyword">fn</span>');
    expect(html).toContain('<span class="hljs-number">1</span>');
  });

  test("leaves a block in no language, or one it does not know, as escaped text", () => {
    for (const fence of ["```", "```klingon"]) {
      const html = renderMarkdown(`${fence}\n<b>if</b> x\n\`\`\``);
      expect(html).toContain("&lt;b&gt;if&lt;/b&gt; x");
      expect(html).not.toContain("hljs-");
    }
  });
});

describe("renderNote", () => {
  const url = (name: string) => (name === "gone.png" ? null : `http://127.0.0.1:4747/files/${name}`);

  test("shows an image the note carries from its address, and marks a link to a file", () => {
    const html = renderNote("![p95](attachment:latency.png) and [the log](attachment:run%20log.txt)", url);
    expect(html).toContain('src="http://127.0.0.1:4747/files/latency.png"');
    expect(html).toContain('data-attachment="latency.png"');
    expect(html).toContain('<a href="#" data-attachment="run log.txt">the log</a>');
  });

  test("leaves out an image it has no address for, and keeps other links", () => {
    const html = renderNote("![gone](attachment:gone.png) [docs](https://example.com)", url);
    expect(html).not.toContain("<img");
    expect(html).toContain('href="https://example.com"');
  });

  test("keeps a file name with a malformed escape as written", () => {
    const html = renderNote("![bad](attachment:a%E0.png) [log](attachment:%FF.log)", url);
    expect(html).toContain('data-attachment="a%E0.png"');
    expect(html).toContain('<a href="#" data-attachment="%FF.log">log</a>');
  });
});

describe("renderLinks", () => {
  test("links web addresses and leaves everything else as text", () => {
    const html = renderLinks("See https://example.com/a_b *now* and <b>this</b> `code`");
    expect(html).toContain('<a href="https://example.com/a_b">https://example.com/a_b</a>');
    expect(html).toContain("*now* and &lt;b&gt;this&lt;/b&gt; `code`");
  });

  test("makes no link of another scheme", () => {
    expect(renderLinks("javascript:alert(1)")).not.toContain("<a ");
  });
});

describe("work item keys", () => {
  test("a key whose prefix is a repository's links to the item", () => {
    setItemPrefixes(["PIN", "PP"]);
    const html = renderMarkdown("Depends on PIN-42 and PP-7, not ZZ-1 or PIN-0.");
    expect(html).toContain('<a href="#/items?open=PIN-42" class="item-ref" data-item="PIN-42">PIN-42</a>');
    expect(html).toContain('<a href="#/items?open=PP-7" class="item-ref" data-item="PP-7">PP-7</a>');
    expect(html).toContain("ZZ-1");
    expect(html).not.toContain('data-item="ZZ-1"');
    expect(html).not.toContain('data-item="PIN-0"');
  });

  test("a key in code or in a link stays as written", () => {
    setItemPrefixes(["PIN"]);
    const html = renderMarkdown("`PIN-1` and [see PIN-2](https://example.com)\n\n```\nPIN-3\n```\n");
    expect(html).not.toContain("data-item");
  });

  test("a note links keys too, and leaves its files' links alone", () => {
    setItemPrefixes(["PIN"]);
    const html = renderNote("Fixed PIN-9, log in [crash](attachment:crash.log).", () => null);
    expect(html).toContain('data-item="PIN-9"');
    expect(html).toContain('data-attachment="crash.log"');
  });

  test("with no prefixes known, a key is plain text, and a change is announced", () => {
    let heard = 0;
    const stop = onItemPrefixes(() => heard++);
    setItemPrefixes([]);
    expect(renderMarkdown("PIN-42")).not.toContain("data-item");
    setItemPrefixes([]);
    stop();
    expect(heard).toBe(1);
  });
});

describe("a diff block", () => {
  const diff = [
    "```diff",
    "diff --git a/src/lib.rs b/src/lib.rs",
    "index 1111111..2222222 100644",
    "--- a/src/lib.rs",
    "+++ b/src/lib.rs",
    "@@ -10,3 +10,3 @@ fn main() {",
    " let a = 1;",
    "-let b = 2;",
    "+let b = 3;",
    " let c = <b>;",
    "\\ No newline at end of file",
    "```",
  ].join("\n");

  test("shows each file, each hunk, and each line with its old and new numbers", () => {
    expect(diffRows(diff.split("\n").slice(1, -1).join("\n"))).toEqual([
      { kind: "file", path: "src/lib.rs" },
      { kind: "hunk", text: "@@ -10,3 +10,3 @@ fn main() {" },
      { kind: "same", old: 10, new: 10, text: "let a = 1;" },
      { kind: "del", old: 11, new: null, text: "let b = 2;" },
      { kind: "add", old: null, new: 11, text: "let b = 3;" },
      { kind: "same", old: 12, new: 12, text: "let c = <b>;" },
      { kind: "note", text: "No newline at end of file" },
    ]);
    const html = renderMarkdown(diff);
    expect(html).toContain('<span class="md-diff-path truncate">src/lib.rs</span>');
    expect(html).toContain(
      '<tr class="md-diff-line md-diff-del"><td class="md-diff-n">11</td><td class="md-diff-n"></td>',
    );
    // highlighted in the file's language, and escaped
    expect(html).toContain('<span class="hljs-keyword">let</span>');
    expect(html).toContain("&lt;b&gt;");
  });

  test("names a deleted file by its old path, and a diff without hunks has no numbers", () => {
    const deleted = renderMarkdown("```diff\n--- a/old.txt\n+++ /dev/null\n@@ -1 +0,0 @@\n-gone\n```");
    expect(deleted).toContain(">old.txt</span>");
    const bare = renderMarkdown("```patch\n-before\n+after\n```");
    expect(bare).not.toContain("md-diff-n");
    expect(bare).toContain("md-diff-del");
    expect(bare).toContain("md-diff-add");
  });

  test("shows one file at a time, with buttons to the others", () => {
    const two = renderMarkdown(
      "```diff\n--- a/a.ts\n+++ b/a.ts\n@@ -1 +1 @@\n-x\n+y\n--- a/b.ts\n+++ b/b.ts\n@@ -1 +1 @@\n-p\n+q\n```",
    );
    expect(two).toContain('data-files="2"');
    // only the first file is drawn; the view keeps the text for the others
    expect(two.match(/<table>/g)).toHaveLength(1);
    expect(two).toContain("data-source=");
    expect(two).toContain('<span class="md-diff-count">1 of 2</span>');
    expect(two).toContain('aria-label="Next file"');
    // one file has no buttons
    expect(renderMarkdown(diff)).not.toContain("data-diff-step");
  });

  test("shows in a note too, and leaves other blocks as code", () => {
    expect(renderNote(diff, () => null)).toContain('class="md-diff"');
    expect(renderMarkdown("```rust\nlet a = 1;\n```")).toContain("<pre>");
  });
});
