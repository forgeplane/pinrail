// The inbox at component level, against the fake core: the notifications in
// their tabs and sections, the person marking them read, unread and done
// with the buttons and the keys, Mark all as read, and the filters.
import { afterEach, describe, expect, test, vi } from "vitest";
import { page, userEvent } from "vitest/browser";
import { useLocation } from "react-router";
import type { Notification, NotificationList, Review } from "../api/types";
import type { NotificationQuery } from "../api/client";
import { core } from "../testing/core";
import { cleanup, render } from "../testing/render";
import { Inbox } from "./Inbox";

vi.mock("../api/client", async (real) => (await import("../testing/core")).fakeClient(await real()));

afterEach(() => {
  cleanup();
  core.reset();
});

const now = () => new Date().toISOString();

const base = (id: number, extra: Partial<Notification>): Notification => ({
  id,
  subject: { kind: "item", item: { id: `w_${id}`, key: `PIN-${id}`, title: `Item ${id}`, status: "open" } },
  repo: "acme/web",
  project: null,
  reason: "item_commented",
  waiting: false,
  actor: { kind: "agent", name: "claude-code", session: null },
  summary: null,
  unread: 1,
  read: false,
  done: false,
  muted: false,
  last_event: 10 + id,
  updated_at: now(),
  ...extra,
});

const update = (id: number, title: string, extra: Partial<Notification> = {}) =>
  base(id, {
    subject: { kind: "item", item: { id: `w_${id}`, key: `PIN-${id}`, title, status: "open" } },
    ...extra,
  });

const waitingReview = (id: number, title: string) =>
  base(id, {
    reason: "review_requested",
    waiting: true,
    subject: {
      kind: "review",
      review: {
        id: `r_${id}`,
        title,
        kind: "review",
        plugin: "list",
        status: "pending",
        origin: { repo: "acme/web" },
        created_at: now(),
      } as unknown as Review,
    },
  });

const waitingAgent = (id: number) =>
  base(id, {
    reason: "permission_requested",
    waiting: true,
    summary: "cargo test",
    subject: {
      kind: "session",
      session: { agent: "claude-code", id: `ses_${id}`, title: "Migrate the billing tables" },
    } as unknown as Notification["subject"],
  });

const listOf = (rows: Notification[], up_to = 40): NotificationList => ({
  notifications: rows,
  total: rows.length,
  counts: { waiting: rows.filter((n) => n.waiting).length, unread: rows.filter((n) => !n.waiting && !n.read).length },
  up_to,
});

/** The inbox at `route`, the core answering each tab from `rows`. */
async function open(rows: Notification[], route = "/") {
  core.answer("listReviews", { reviews: [], total: 0 });
  core.answer("plugins", { plugins: [] });
  core.answer("notificationCounts", listOf(rows).counts);
  core.answer("notifications", (query: unknown) => {
    const tab = (query as NotificationQuery).tab;
    const kept = rows.filter((n) =>
      tab === "waiting" ? n.waiting : tab === "updates" ? !n.waiting : tab === "unread" ? !n.read : true,
    );
    return { ...listOf(kept), counts: listOf(rows).counts };
  });
  for (const write of ["setNotificationRead", "setNotificationDone", "readNotifications"] as const) {
    core.answer(write, undefined);
  }
  await render(
    <>
      <Inbox />
      <Where />
    </>,
    { route },
  );
}

/** The address the inbox is at, for the test to read. */
function Where() {
  const { search } = useLocation();
  return <output data-testid="where">{search}</output>;
}

const row = (title: string) => page.getByTestId("update-row").filter({ hasText: title });
const lastQuery = () => core.calls("notifications").at(-1)?.[0] as NotificationQuery;

describe("the inbox", () => {
  test("All shows what waits on the person first, and the updates below", async () => {
    await open([update(1, "Load themes"), waitingReview(2, "Rate limiter"), waitingAgent(3)]);
    const waiting = page.getByRole("region", { name: "Waiting on you" });
    await expect.element(waiting.getByTestId("review-row")).toMatchTextContent("Rate limiter");
    await expect.element(waiting.getByTestId("update-row")).toMatchTextContent("An agent waits for permission");
    await expect
      .element(page.getByRole("region", { name: "Updates" }).getByTestId("update-row"))
      .toMatchTextContent("claude-code commented");
    await expect.element(page.getByRole("tab", { name: /Waiting on you/ })).toMatchTextContent("2");
  });

  test("a notification that waits offers no Mark as done", async () => {
    await open([waitingAgent(3)]);
    await expect.element(row("Migrate the billing tables")).toBeVisible();
    await expect.element(page.getByRole("button", { name: /Mark as read: / })).toBeInTheDocument();
    await expect.element(page.getByRole("button", { name: /Mark as done/ })).not.toBeInTheDocument();
  });

  test("the buttons mark an update read, unread and done", async () => {
    await open([update(1, "Load themes"), update(4, "Docs", { read: true, unread: 0 })]);
    await page.getByRole("button", { name: "Mark as read: Load themes" }).click();
    expect(core.calls("setNotificationRead").at(-1)).toEqual([1, true]);
    await page.getByRole("button", { name: "Mark as unread: Docs" }).click();
    expect(core.calls("setNotificationRead").at(-1)).toEqual([4, false]);
    await page.getByRole("button", { name: "Mark as done: Load themes" }).click();
    expect(core.calls("setNotificationDone").at(-1)).toEqual([1, true]);
  });

  test("Mark all as read covers what the tab and the filters list", async () => {
    await open([update(1, "Load themes"), update(2, "Docs")], "/?kind=item_commented,item_added");
    await page.getByRole("button", { name: "Mark all as read" }).click();
    const [filters, upTo] = core.calls("readNotifications").at(-1)!;
    expect(filters).toMatchObject({ tab: "all", kind: ["item_commented", "item_added"] });
    expect(upTo).toBe(40);
  });

  test("U, E and Shift-U act on the focused notification", async () => {
    await open([update(1, "Load themes")]);
    await row("Load themes").hover();
    await expect.element(row("Load themes")).toHaveClass(/is-focused/);
    await userEvent.keyboard("u");
    expect(core.calls("setNotificationRead").at(-1)).toEqual([1, true]);
    await userEvent.keyboard("e");
    expect(core.calls("setNotificationDone").at(-1)).toEqual([1, true]);
    await userEvent.keyboard("{Shift>}U{/Shift}");
    expect(core.calls("readNotifications").at(-1)?.[1]).toBe(40);
  });

  test("the tabs ask the core for their own list", async () => {
    await open([update(1, "Load themes"), waitingReview(2, "Rate limiter")]);
    await page.getByRole("tab", { name: /Unread/ }).click();
    await vi.waitFor(() => expect(lastQuery().tab).toBe("unread"));
    await page.getByRole("tab", { name: /Waiting on you/ }).click();
    await vi.waitFor(() => expect(lastQuery().tab).toBe("waiting"));
  });

  test("the Kind filter takes several values, and shows one chip that names them", async () => {
    await open([update(1, "Load themes")]);
    await page.getByRole("button", { name: "Filter" }).click();
    await page.getByRole("menuitem", { name: /Kind/ }).hover();
    await page.getByRole("menuitemcheckbox", { name: "Comments" }).click();
    // the menu stays open for the next choice
    await page.getByRole("menuitemcheckbox", { name: "New work items" }).click();
    await vi.waitFor(() => expect(lastQuery().kind).toEqual(["item_commented", "item_added"]));
    await userEvent.keyboard("{Escape}");
    await expect.element(page.getByTestId("filter-chip")).toMatchTextContent("Comments, New work items");
  });

  test("the address of the old New rounds tab opens the Kind filter on new rounds", async () => {
    await open([], "/?rounds=new");
    await expect.element(page.getByTestId("where")).toMatchTextContent("kind=new_round");
  });
});
