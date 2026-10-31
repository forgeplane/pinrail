import { describe, expect, test } from "vitest";
import { renderMarkdown } from "./markdown";

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
