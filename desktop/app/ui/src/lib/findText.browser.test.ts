// Find in page over real layout, in WebKit: which text counts, how a match
// may cross inline elements, and how the current match moves.
import { afterEach, describe, expect, test } from "vitest";
import { FIND_LIMIT, finder, type Found } from "./findText";

let root: HTMLElement;

/** A root with this HTML in the page, and its finder. */
function setup(html: string) {
  root = document.createElement("div");
  root.innerHTML = html;
  document.body.append(root);
  const changes: Found[] = [];
  const find = finder(root, (found) => changes.push(found));
  return { find, changes };
}

const current = () => [...(CSS.highlights.get("pinrail-find-current") ?? [])].map((r) => (r as Range).toString());
const all = () => [...(CSS.highlights.get("pinrail-find") ?? [])].map((r) => (r as Range).toString());

afterEach(() => root.remove());

describe("finder", () => {
  test("counts the matches ignoring case, and highlights them with the first current", () => {
    const { find } = setup("<p>Deploy the app.</p><p>The deploy failed; deploy again.</p>");
    expect(find.find("deploy")).toEqual({ count: 3, current: 1 });
    expect(all()).toEqual(["Deploy", "deploy", "deploy"]);
    expect(current()).toEqual(["Deploy"]);
    find.stop();
  });

  test("the same search again moves to the next match, and backwards to the previous, round", () => {
    const { find } = setup("<p>one a</p><p>two a</p><p>three a</p>");
    expect(find.find("a").current).toBe(1);
    expect(find.find("a").current).toBe(2);
    expect(find.find("a").current).toBe(3);
    expect(find.find("a").current).toBe(1);
    expect(find.find("a", true).current).toBe(3);
    find.stop();
  });

  test("a match crosses inline elements of one block, not two blocks", () => {
    const { find } = setup('<p>see <a href="#">https://example</a>.com/docs</p><p>see</p><p>https</p>');
    expect(find.find("example.com").count).toBe(1);
    expect(all()).toEqual(["example.com"]);
    // "see" ends one block and "https" starts the next: no match across them
    expect(find.find("seehttps").count).toBe(0);
    find.stop();
  });

  test("text with no box, and the text of fields and scripts, does not count", () => {
    const { find } = setup(
      '<p>visible word</p><p hidden>hidden word</p><p style="display:none">gone word</p>' +
        "<textarea>typed word</textarea><input value='field word'>",
    );
    expect(find.find("word").count).toBe(1);
    find.stop();
  });

  test("a query is matched as it is written, not as a pattern", () => {
    const { find } = setup("<p>a.b and axb, (1+1)</p>");
    expect(find.find("a.b").count).toBe(1);
    expect(find.find("(1+1)").count).toBe(1);
    find.stop();
  });

  test("an empty query clears the search and its highlights", () => {
    const { find } = setup("<p>deploy</p>");
    find.find("deploy");
    expect(find.find("")).toEqual({ count: 0, current: 0 });
    expect(all()).toEqual([]);
    find.stop();
  });

  test("counts no more than the limit", () => {
    const { find } = setup(`<p>${"x ".repeat(FIND_LIMIT + 50)}</p>`);
    expect(find.find("x").count).toBe(FIND_LIMIT);
    find.stop();
  });

  test("text added while a search shows is counted, and the change reported", async () => {
    const { find, changes } = setup("<p>deploy</p>");
    expect(find.find("deploy").count).toBe(1);
    const more = document.createElement("p");
    more.textContent = "deploy again";
    root.append(more);
    await expect.poll(() => changes.at(-1)).toEqual({ count: 2, current: 1 });
    // after stop, a change is not watched
    find.stop();
    root.append(more.cloneNode(true));
    await new Promise((resolve) => setTimeout(resolve, 300));
    expect(changes).toHaveLength(1);
  });
});
