// Markdown tables in a session's thread, as agents write them: a work
// item's key stays whole in a narrow column.
import "../../theme.css";
import { afterEach, expect, test } from "vitest";
import { setItemPrefixes, renderMarkdown } from "../../lib/markdown";
import { cleanup, render } from "../../testing/render";

afterEach(() => cleanup());

const table = [
  "| Item | What it is |",
  "| --- | --- |",
  "| PM-1 | **First series of Pinrail shorts**, the parent item. |",
  "| PM-10 | **Write a plugin for your own reviews** (45 s). `pinrail plugins new`: a manifest, two schemas and an HTML view, with no build step. |",
].join("\n");

test.each(["bordered", "lines", "striped"])("a work item's key stays on one line in a %s table", async (style) => {
  document.documentElement.dataset.tables = style;
  document.documentElement.dataset.tableWidth = "full";
  setItemPrefixes(["PM"]);
  await render(
    <div
      className="thread-note-body"
      style={{ width: 420 }}
      dangerouslySetInnerHTML={{ __html: renderMarkdown(table) }}
    />,
  );
  const keys = [...document.querySelectorAll<HTMLElement>("a.item-ref")];
  expect(keys.map((k) => k.textContent)).toEqual(["PM-1", "PM-10"]);
  for (const key of keys) expect(key.getClientRects(), key.textContent!).toHaveLength(1);
});
