import { describe, expect, test } from "vitest";
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
