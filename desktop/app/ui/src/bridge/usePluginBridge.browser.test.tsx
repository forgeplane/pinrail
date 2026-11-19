// The bridge between the app and a plugin's view, with a stand-in view in a
// real frame: it says it is ready, shows the review it is given, answers a
// request for its decision, and shows what the app says back.
import { useRef, useState } from "react";
import { afterEach, describe, expect, test, vi } from "vitest";
import { page, userEvent } from "vitest/browser";
import type { Review } from "../api/types";
import { core } from "../testing/core";
import { cleanup, render } from "../testing/render";
import { usePluginBridge, type SubmitResult } from "./usePluginBridge";

vi.mock("../api/client", async (real) => (await import("../testing/core")).fakeClient(await real()));

afterEach(() => {
  cleanup();
  core.reset();
  sessionStorage.clear();
});

/** A view that speaks the plugin protocol by hand, as the SDK would. */
const VIEW = `<!doctype html><body><p id="title">waiting</p><p id="heard"></p><script>
  const send = (m) => parent.postMessage({ pinrail: 1, ...m }, "*");
  addEventListener("message", (e) => {
    const m = e.data;
    if (!m || m.pinrail !== 1) return;
    if (m.type === "init") document.getElementById("title").textContent = m.review.title;
    else if (m.type === "collect") send({ type: "submit", req: m.req, data: { approved: true } });
    else if (m.type === "submitted" || m.type === "violations") document.getElementById("heard").textContent = m.type;
  });
  send({ type: "ready" });
</script></body>`;

const review = {
  id: "r_1",
  kind: "review",
  plugin: "stand-in",
  title: "Approve the plan",
  status: "pending",
  created_at: "2026-11-19T09:00:00Z",
  payload: {},
} as unknown as Review;

function View({
  onSubmit,
  readonly = false,
}: {
  onSubmit: (data: unknown) => Promise<SubmitResult>;
  readonly?: boolean;
}) {
  const frame = useRef<HTMLIFrameElement>(null);
  const [src] = useState(() => URL.createObjectURL(new Blob([VIEW], { type: "text/html" })));
  const bridge = usePluginBridge({
    frame,
    reviewId: review.id,
    review,
    previous: null,
    readonly,
    src,
    connected: true,
    onSubmit,
    settings: null,
    onSetSetting: async () => [],
    onOpen: () => {},
  });
  return (
    <>
      <iframe ref={frame} title="view" />
      <button type="button" onClick={bridge.collect}>
        Hand over
      </button>
      <output>{bridge.loaded ? "loaded" : "loading"}</output>
    </>
  );
}

const view = () => page.frameLocator(page.getByTitle("view"));

describe("the plugin bridge", () => {
  test("a view that says it is ready is given the review", async () => {
    await render(<View onSubmit={vi.fn()} />);
    await expect.element(view().getByText("Approve the plan")).toBeVisible();
    await expect.element(page.getByText("loaded")).toBeVisible();
  });

  test("a hand-over asks the view for its decision, and tells it the decision was taken", async () => {
    const onSubmit = vi.fn(async () => ({ ok: true, decision: { data: { approved: true } } }) as SubmitResult);
    await render(<View onSubmit={onSubmit} />);
    await expect.element(page.getByText("loaded")).toBeVisible();
    await userEvent.click(page.getByRole("button", { name: "Hand over" }));
    await expect.poll(() => onSubmit.mock.calls).toEqual([[{ approved: true }]]);
    await expect.element(view().getByText("submitted")).toBeVisible();
  });

  test("a decision the core refuses is sent back to the view as violations", async () => {
    const onSubmit = vi.fn(
      async () => ({ ok: false, violations: [{ path: "/approved", message: "must be a string" }] }) as SubmitResult,
    );
    await render(<View onSubmit={onSubmit} />);
    await expect.element(page.getByText("loaded")).toBeVisible();
    await userEvent.click(page.getByRole("button", { name: "Hand over" }));
    await expect.element(view().getByText("violations")).toBeVisible();
  });

  test("a read-only view hands nothing over", async () => {
    const onSubmit = vi.fn();
    await render(<View onSubmit={onSubmit} readonly />);
    await expect.element(page.getByText("loaded")).toBeVisible();
    await userEvent.click(page.getByRole("button", { name: "Hand over" }));
    await new Promise((resolve) => setTimeout(resolve, 300));
    expect(onSubmit).not.toHaveBeenCalled();
  });

  test("a message from another window than the frame's is not heard", async () => {
    const onSubmit = vi.fn();
    await render(<View onSubmit={onSubmit} />);
    await expect.element(page.getByText("loaded")).toBeVisible();
    window.postMessage({ pinrail: 1, type: "submit", req: 1, data: { forged: true } }, "*");
    await new Promise((resolve) => setTimeout(resolve, 300));
    expect(onSubmit).not.toHaveBeenCalled();
  });
});
