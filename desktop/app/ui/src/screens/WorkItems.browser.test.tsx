// The work items page at component level, against the fake core: what it
// does with its keys, its filters and the core's notices. What the core
// answers, such as which items a status returns or the queue's order, stays
// in the end-to-end specs (e2e/shell/items/work-items.spec.ts).
import { afterEach, describe, expect, test, vi } from "vitest";
import { page, userEvent } from "vitest/browser";
import { useLocation } from "react-router";
import type { Notice, WorkItem } from "../api/types";
import { core } from "../testing/core";
import { workItem } from "../testing/items";
import { cleanup, render } from "../testing/render";
import { WorkItems } from "./WorkItems";

vi.mock("../api/client", async (real) => (await import("../testing/core")).fakeClient(await real()));

afterEach(() => {
  cleanup();
  core.reset();
});

/** The address the page is at, shown for the test to read. */
function Where() {
  const { search } = useLocation();
  return <output data-testid="where">{search}</output>;
}

/** The page at `/items`, listing `items`, and its rows once they show. */
async function open(items: WorkItem[], search = "") {
  core.answer("items", { items });
  core.answer("editItem", (key: unknown) => ({ item: items.find((i) => i.key === key) }));
  core.answer("queueItem", { items: [] });
  await render(
    <>
      <WorkItems />
      <Where />
    </>,
    { route: `/items${search}` },
  );
  await expect.element(page.getByTestId("item-row").first()).toBeVisible();
}

const row = (key: string) => page.getByTestId("item-row").filter({ hasText: key });
const where = () => page.getByTestId("where");

describe("the list's keys", () => {
  test("J and K move the focus over the rows, from the first, and Enter opens the focused one", async () => {
    await open([workItem({ title: "One", number: 1 }), workItem({ title: "Two", number: 2 })]);
    await expect.element(row("ACME-1")).toHaveClass(/is-focused/);
    await userEvent.keyboard("j");
    await expect.element(row("ACME-2")).toHaveClass(/is-focused/);
    await userEvent.keyboard("k");
    await expect.element(row("ACME-1")).toHaveClass(/is-focused/);
    await userEvent.keyboard("{Enter}");
    await expect.element(where()).toMatchTextContent("open=ACME-1");
  });

  test("D marks the focused item done, and S claims it", async () => {
    await open([workItem({ title: "One", number: 1 })]);
    await userEvent.keyboard("d");
    await expect.poll(() => core.calls("editItem")).toContainEqual(["ACME-1", { status: "done" }]);
    await userEvent.keyboard("s");
    await expect.poll(() => core.calls("editItem")).toContainEqual(["ACME-1", { claim: "take" }]);
  });

  test("Q queues the focused item", async () => {
    await open([workItem({ title: "One", number: 1 })]);
    await userEvent.keyboard("q");
    await expect.poll(() => core.calls("queueItem")).toContainEqual(["ACME-1"]);
  });

  test("X cancels an item at once, but asks first for one with open sub-items", async () => {
    await open([
      workItem({ title: "Alone", number: 1 }),
      workItem({ title: "Parent", number: 2, sub_items: { total: 2, closed: 0, in_progress: 0 } }),
    ]);
    await userEvent.keyboard("x");
    await expect.poll(() => core.calls("editItem")).toContainEqual(["ACME-1", { status: "cancelled" }]);

    await userEvent.keyboard("j");
    await userEvent.keyboard("x");
    const dialog = page.getByRole("dialog", { name: "Cancel ACME-2?" });
    await expect.element(dialog).toBeVisible();
    await expect.element(dialog.getByRole("button", { name: "Keep it" })).toHaveFocus();
    await userEvent.click(dialog.getByRole("button", { name: "Cancel them all" }));
    await expect.poll(() => core.calls("editItem")).toContainEqual(["ACME-2", { status: "cancelled" }]);
  });

  test("the keys are left alone while the search field has the focus", async () => {
    await open([workItem({ title: "One", number: 1 })]);
    await userEvent.keyboard("/");
    await userEvent.keyboard("dsx");
    expect(core.calls("editItem")).toEqual([]);
  });
});

describe("the filters", () => {
  test("a status picked in the Filter menu asks the core for that status and shows as a chip", async () => {
    await open([workItem({ title: "One", number: 1 })]);
    await userEvent.click(page.getByRole("button", { name: "Filter", exact: true }));
    await userEvent.click(page.getByRole("menuitem", { name: /^Status/ }));
    await userEvent.click(page.getByRole("menuitemradio", { name: "Done", exact: true }));
    await expect.element(where()).toMatchTextContent("status=done");
    await expect.poll(() => core.calls("items")).toContainEqual([expect.objectContaining({ status: "done" })]);
  });

  test("with no item, says there are none yet; with a filter, that none match", async () => {
    await render(<WorkItems />, { route: "/items" });
    core.answer("items", { items: [] });
    // answered after the first load: a notice loads the list again
    core.notice({ kind: "item_changed", item: { key: "ACME-9" } } as unknown as Notice);
    await expect.element(page.getByText("No work items yet")).toBeVisible();
    cleanup();
    await render(<WorkItems />, { route: "/items?status=done" });
    await expect.element(page.getByText("No matching work items")).toBeVisible();
  });
});

describe("sub-items", () => {
  test("show under their parent, which hides and shows them", async () => {
    await open([
      workItem({ title: "Parent", number: 1, sub_items: { total: 1, closed: 0, in_progress: 0 } }),
      workItem({ title: "Child", number: 2, parent: "ACME-1" }),
    ]);
    await expect.element(row("ACME-2")).toBeVisible();
    await userEvent.click(page.getByRole("button", { name: "Hide the sub-items of ACME-1" }));
    await expect.element(row("ACME-2")).not.toBeInTheDocument();
    await userEvent.click(page.getByRole("button", { name: "Show the sub-items of ACME-1" }));
    await expect.element(row("ACME-2")).toBeVisible();
  });
});

describe("the core's notices", () => {
  test("a change to an item loads the list again, and shows its new title", async () => {
    await open([workItem({ title: "One", number: 1 })]);
    // changed, as its time says: the list draws again only the rows that did
    core.answer("items", {
      items: [workItem({ title: "One, renamed", number: 1, updated_at: "2026-11-19T10:00:00Z" })],
    });
    core.notice({ kind: "item_changed", item: { key: "ACME-1" } } as unknown as Notice);
    await expect.element(row("ACME-1")).toMatchTextContent("One, renamed");
  });
});
