import { describe, expect, test } from "vitest";
import { renderLinks, renderMarkdown, renderNote } from "./markdown";

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
