// A review in the inbox's side panel, at component level, against the fake
// core: its keys.
import { useEffect, useState } from "react";
import { afterEach, expect, test, vi } from "vitest";
import { page, userEvent } from "vitest/browser";
import type { Review } from "../api/types";
import { core } from "../testing/core";
import { cleanup, render } from "../testing/render";
import { ReviewView } from "./ReviewScreen";

vi.mock("../api/client", async (real) => (await import("../testing/core")).fakeClient(await real()));

afterEach(() => {
  cleanup();
  core.reset();
});

const review = {
  id: "r_1",
  kind: "review",
  plugin: "list",
  plugin_version: "1.0.0",
  plugin_bundle: "b1",
  title: "Approve the plan",
  origin: { repo: "acme/app" },
  status: "pending",
  created_at: "2026-11-19T09:00:00Z",
  revises: null,
  payload: {},
} as unknown as Review;

/** A page that draws again on every key press, from a key listener added
 *  before the review's: React then draws between the listeners of one key
 *  press, and gives the review new props. */
function Page() {
  const [open, setOpen] = useState(true);
  const [, setPressed] = useState(0);
  useEffect(() => {
    const onKey = () => setPressed((n) => n + 1);
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);
  return open ? <ReviewView id="r_1" inPanel onClose={() => setOpen(false)} /> : <p>Closed</p>;
}

test("Esc closes the panel when the page draws again during the key press", async () => {
  core.answer("getReview", review);
  core.answer("rounds", []);
  core.answer("markViewed", undefined);
  await render(<Page />);
  await expect.element(page.getByText("Approve the plan").first()).toBeVisible();
  await userEvent.keyboard("{Escape}");
  await expect.element(page.getByText("Closed")).toBeVisible();
});
