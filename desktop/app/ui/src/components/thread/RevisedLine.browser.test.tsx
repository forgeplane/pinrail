// The line where the agent sent a later round of a review, in the app's
// styles: its Waiting for you badge stays whole when the line wraps.
import { expect, test } from "vitest";
import { page } from "vitest/browser";
import type { Review } from "../../api/types";
import { render } from "../../testing/render";
import { RevisedLine } from "./messages";
import "../../theme.css";

test("the Waiting for you badge moves to the next line whole, rather than breaking", async () => {
  const review = {
    id: "r_2",
    title: "PIN-795 step 1: connect and remove agents through their adapters",
    status: "pending",
    created_at: new Date().toISOString(),
  } as unknown as Review;
  await render(
    <div className="thread" data-testid="box" style={{ width: 560 }}>
      <RevisedLine entry={{ kind: "revised", review, round: 2, card: review }} />
    </div>,
  );
  const badge = page.getByText("Waiting for you");
  await expect.element(badge).toBeVisible();
  // at every width the line wraps somewhere; a badge broken across two
  // lines draws two boxes
  const box = page.getByTestId("box").element() as HTMLElement;
  const broken: number[] = [];
  for (let width = 300; width <= 700; width += 10) {
    box.style.width = `${width}px`;
    if (badge.element().getClientRects().length !== 1) broken.push(width);
  }
  expect(broken).toEqual([]);
});
