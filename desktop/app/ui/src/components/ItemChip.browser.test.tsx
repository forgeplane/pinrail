// A work item's chip with the app's styles and fonts, as the window shows it.
import { afterEach, expect, test } from "vitest";
import { cleanup, render } from "../testing/render";
import { ItemChip } from "./ItemChip";

afterEach(cleanup);

/** Where the text of `el` sits: the bottom of an empty inline block put at
 *  its end, which lies on the text's baseline. */
function baseline(el: Element) {
  const mark = document.createElement("span");
  mark.style.display = "inline-block";
  el.append(mark);
  const y = mark.getBoundingClientRect().bottom;
  mark.remove();
  return y;
}

test("the chip's key and title share a baseline", async () => {
  await import("@fontsource-variable/inter/wght.css");
  await import("../theme.css");
  await render(<ItemChip item={{ key: "PIN-701", title: "Marketing shorts character" }} />, { bare: true });
  await document.fonts.ready;
  const key = document.querySelector(".item-chip .mono")!;
  const title = document.querySelector(".item-chip-title")!;
  expect(baseline(key)).toBeCloseTo(baseline(title), 1);
});
