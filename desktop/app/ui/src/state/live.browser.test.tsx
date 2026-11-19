// The live state at component level, against the fake core: the connection
// as the event stream opens, breaks and opens again, and the pending
// reviews as notices change them.
import { afterEach, describe, expect, test, vi } from "vitest";
import { page } from "vitest/browser";
import type { Notice, Review } from "../api/types";
import { core } from "../testing/core";
import { cleanup, render } from "../testing/render";
import { useLive } from "./live";

vi.mock("../api/client", async (real) => (await import("../testing/core")).fakeClient(await real()));

afterEach(() => {
  cleanup();
  core.reset();
});

const review = (id: string, title: string) =>
  ({
    id,
    title,
    kind: "review",
    plugin: "list",
    status: "pending",
    origin: {},
    created_at: "2026-11-19T09:00:00Z",
  }) as unknown as Review;

function Probe() {
  const live = useLive();
  return (
    <p>
      {live.connected ? "Connected" : "Disconnected"}: {live.pending.map((r) => r.title).join(", ") || "none"}
    </p>
  );
}

async function open(pending: Review[]) {
  core.answer("listReviews", () => ({ reviews: pending, total: pending.length }));
  core.answer("plugins", { plugins: [] });
  await render(<Probe />);
  return pending;
}

describe("the live state", () => {
  test("connects, says so when the stream breaks, and loads the pending reviews again when it is back", async () => {
    const pending = await open([review("r_1", "First")]);
    await expect.element(page.getByText("Connected: First")).toBeVisible();
    core.drop();
    await expect.element(page.getByText("Disconnected: First")).toBeVisible();
    // what changed while it was away arrives with the next load
    pending.push(review("r_2", "Second"));
    const loads = core.calls("listReviews").length;
    core.reopen();
    await expect.element(page.getByText(/^Connected: (First, Second|Second, First)$/)).toBeVisible();
    expect(core.calls("listReviews").length).toBeGreaterThan(loads);
  });

  test("a review created, then decided, joins and leaves the pending reviews without a load", async () => {
    await open([review("r_1", "First")]);
    await expect.element(page.getByText("Connected: First")).toBeVisible();
    const loads = core.calls("listReviews").length;
    core.notice({ kind: "created", review_id: "r_2", review: review("r_2", "Second") } as unknown as Notice);
    await expect.element(page.getByText("Connected: Second, First")).toBeVisible();
    core.notice({ kind: "decided", review_id: "r_1" } as unknown as Notice);
    await expect.element(page.getByText("Connected: Second")).toBeVisible();
    expect(core.calls("listReviews").length).toBe(loads);
  });
});
