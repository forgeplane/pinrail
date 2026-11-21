// A work item's side panel at component level, against the fake core:
// editing in place, the queue pill, comments, and closing.
import { afterEach, describe, expect, test, vi } from "vitest";
import { page, userEvent } from "vitest/browser";
import { ApiError } from "../api/client";
import type { ItemDetail } from "../api/types";
import { core } from "../testing/core";
import { itemDetail } from "../testing/items";
import { cleanup, render } from "../testing/render";
import { ItemPanel } from "./ItemPanel";

vi.mock("../api/client", async (real) => (await import("../testing/core")).fakeClient(await real()));

afterEach(() => {
  cleanup();
  core.reset();
});

/** The panel of `item`, which the fake core keeps and changes as the real
 *  one would for the calls these tests make. */
async function open(item: ItemDetail) {
  let held = item;
  core.answer("item", () => held);
  core.answer("editItem", (_key: unknown, body: unknown) => {
    held = { ...held, ...(body as Partial<ItemDetail>), updated_at: new Date().toISOString() };
    return { item: held };
  });
  core.answer("queueItem", () => {
    held = { ...held, queue: 2 };
    return { items: [] };
  });
  core.answer("commentItem", {});
  const props = { onOpen: vi.fn(), onClose: vi.fn(), onChanged: vi.fn() };
  await render(<ItemPanel itemKey={item.key} items={[]} repos={[item.repo]} {...props} />, { route: "/items" });
  await expect.element(page.getByTestId("item-panel")).toMatchTextContent(item.title);
  return props;
}

const panel = () => page.getByTestId("item-panel");

describe("the item panel", () => {
  test("T edits the title in place, and Enter saves it", async () => {
    const { onChanged } = await open(itemDetail({ title: "Fix the login", number: 7 }));
    await userEvent.keyboard("t");
    const field = page.getByRole("textbox", { name: "Title" });
    await expect.element(field).toHaveFocus();
    await userEvent.fill(field, "Fix the login redirect");
    await userEvent.keyboard("{Enter}");
    await expect.poll(() => core.calls("editItem")).toEqual([["ACME-7", { title: "Fix the login redirect" }]]);
    await expect.element(panel()).toMatchTextContent("Fix the login redirect");
    expect(onChanged).toHaveBeenCalled();
  });

  test("Esc leaves the title unsaved, and Esc again closes the panel", async () => {
    const { onClose } = await open(itemDetail({ title: "Fix the login", number: 8 }));
    await userEvent.keyboard("t");
    await userEvent.fill(page.getByRole("textbox", { name: "Title" }), "Something else");
    await userEvent.keyboard("{Escape}");
    await expect.element(page.getByRole("textbox", { name: "Title" })).not.toBeInTheDocument();
    expect(core.calls("editItem")).toEqual([]);
    expect(onClose).not.toHaveBeenCalled();
    await userEvent.keyboard("{Escape}");
    expect(onClose).toHaveBeenCalled();
  });

  test("the queue pill queues the item and names its place", async () => {
    await open(itemDetail({ title: "Queue me", number: 9 }));
    await userEvent.click(panel().getByRole("button", { name: "Queue", exact: true }));
    await expect.poll(() => core.calls("queueItem")).toEqual([["ACME-9"]]);
    await expect.element(panel().getByRole("button", { name: "Queued · 2nd" })).toBeVisible();
  });

  test("the button beside the parent opens the parent", async () => {
    const { onOpen } = await open(itemDetail({ title: "A sub-item", number: 12, parent: "ACME-3" }));
    await userEvent.click(panel().getByRole("button", { name: "Open the parent" }));
    expect(onOpen).toHaveBeenCalledWith("ACME-3");
  });

  test("an item without a parent has no button to open one", async () => {
    await open(itemDetail({ title: "On its own", number: 13 }));
    await expect.element(panel().getByRole("button", { name: "Open the parent" })).not.toBeInTheDocument();
  });

  test("a closed item has no queue pill", async () => {
    await open(itemDetail({ title: "Done already", number: 10, status: "done" }));
    await expect.element(panel().getByRole("button", { name: /^Queue/ })).not.toBeInTheDocument();
  });

  test("a comment is posted with the comment key, and the field empties", async () => {
    await open(itemDetail({ title: "Talk about it", number: 11 }));
    const box = page.getByRole("textbox", { name: "Add a comment" });
    await userEvent.fill(box, "Looks right to me");
    await userEvent.keyboard("{ControlOrMeta>}{Enter}{/ControlOrMeta}");
    await expect.poll(() => core.calls("commentItem")).toEqual([["ACME-11", "Looks right to me", {}]]);
    await expect.element(box).toHaveValue("");
  });

  test("an item that is gone says so", async () => {
    core.answer("item", () => Promise.reject(new ApiError(404, { error: "not_found", message: "no such item" })));
    await render(
      <ItemPanel itemKey="ACME-99" items={[]} repos={[]} onOpen={vi.fn()} onClose={vi.fn()} onChanged={vi.fn()} />,
    );
    await expect.element(page.getByText("This work item is no longer here.")).toBeVisible();
  });
});
