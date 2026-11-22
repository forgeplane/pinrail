// The inbox at component level, against the fake core: the notifications in
// their tabs and sections, the person marking them read, unread and done
// with the buttons and the keys, Mark all as read, and the filters.
import { afterEach, describe, expect, test, vi } from "vitest";
import { page, userEvent } from "vitest/browser";
import { useLocation } from "react-router";
import type { Notification, NotificationList, Review } from "../api/types";
import type { NotificationQuery } from "../api/client";
import { core } from "../testing/core";
import { itemDetail } from "../testing/items";
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
  read_event: 0,
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
  for (const write of [
    "setNotificationRead",
    "setNotificationDone",
    "setNotificationMuted",
    "readNotifications",
  ] as const) {
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
  const { pathname, search } = useLocation();
  return <output data-testid="where">{pathname + decodeURIComponent(search)}</output>;
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

  test("each row has its subject's icon, with what happened as a badge", async () => {
    const done = update(4, "Ship it", { reason: "item_closed" });
    (done.subject as { item: { status: string } }).item.status = "done";
    const cancelled = update(5, "Drop it", { reason: "item_closed" });
    (cancelled.subject as { item: { status: string } }).item.status = "cancelled";
    await open([
      update(1, "Load themes", { reason: "item_added" }),
      update(2, "Read themes", { reason: "item_claimed" }),
      update(3, "Save themes"),
      done,
      cancelled,
      waitingReview(6, "Rate limiter"),
      waitingAgent(7),
    ]);
    const shows = async (title: string, name: string) =>
      expect.element(row(title).getByRole("img", { name, exact: true }).first()).toBeVisible();
    await shows("Load themes", "Added");
    await shows("Read themes", "Claimed");
    await shows("Save themes", "Commented");
    await shows("Drop it", "Cancelled");
    await shows("Migrate the billing tables", "Waits for a permission");
    // a work item's status follows its title: the badge and the ring both say Done
    expect(row("Ship it").getByRole("img", { name: "Done", exact: true }).elements()).toHaveLength(2);
    // a waiting review says what happened, as an update does
    await expect
      .element(page.getByTestId("review-row").filter({ hasText: "Rate limiter" }))
      .toMatchTextContent(/requested a review|A review for you/);
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

  test("a row says what changed since the person looked, and has no arrow", async () => {
    const fresh = (extra: object) => ({
      sub_items: 0,
      sub_item: null,
      breakdown: false,
      comments: 0,
      authors: [],
      comment: null,
      changes: 0,
      latest: null,
      parent: null,
      turns: 0,
      reply: null,
      ...extra,
    });
    await open([
      update(1, "First series of shorts", {
        reason: "item_added",
        new: fresh({ latest: "sub_item", breakdown: true, sub_items: 10 }),
      }),
      update(2, "Make the inbox a notification center", {
        reason: "item_added",
        new: fresh({ latest: "sub_item", sub_items: 1, sub_item: { key: "PIN-708", title: "Show a new sub-item" } }),
      }),
      update(3, "Turn the inbox into a notification center", {
        new: fresh({ latest: "comment", comments: 3, authors: ["claude-code", "codex"], changes: 1 }),
      }),
      update(4, "Resume a stopped agent", {
        new: fresh({
          latest: "comment",
          comments: 1,
          authors: ["codex"],
          comment: "The flag differs.",
          parent: "PIN-301",
        }),
      }),
      {
        ...waitingAgent(5),
        waiting: false,
        reason: "agent_finished",
        summary: null,
        new: fresh({ turns: 3, reply: "Pushed the fix." }),
      },
    ]);
    await expect.element(row("First series")).toMatchTextContent("claude-code added this work item with 10 sub-items");
    await expect
      .element(row("Make the inbox"))
      .toMatchTextContent("claude-code added a sub-item·PIN-708 Show a new sub-item");
    await expect
      .element(row("Turn the inbox"))
      .toMatchTextContent("3 new comments·claude-code, codex·and 1 more change");
    await expect.element(row("Resume a stopped")).toMatchTextContent("codex commented·The flag differs.·Under PIN-301");
    await expect.element(row("Migrate the billing")).toMatchTextContent("The agent finished 3 turns·Pushed the fix.");
    await expect.element(page.getByRole("button", { name: /Show the new events/ })).not.toBeInTheDocument();
  });

  test("a work item's and a session's notification open beside the list", async () => {
    await open([update(1, "Load themes"), waitingAgent(3)]);
    await row("Load themes").click();
    await expect.element(page.getByTestId("where")).toHaveTextContent("/?open=item:PIN-1");
    await expect.element(page.getByRole("complementary", { name: "Work item" })).toBeInTheDocument();
    expect(core.calls("setNotificationRead").at(-1)).toEqual([1, true]);
    await row("Migrate the billing tables").click();
    await expect.element(page.getByTestId("where")).toHaveTextContent("/?open=session:claude-code/ses_3");
    await expect.element(page.getByRole("complementary", { name: "Session" })).toBeInTheDocument();
  });

  test("a row is muted, with its button or M, and unmuted with the bell on the work item's panel", async () => {
    await open([update(1, "Load themes")]);
    await page.getByRole("button", { name: "Mute: Load themes" }).click();
    expect(core.calls("setNotificationMuted").at(-1)).toEqual([1, true]);
    await row("Load themes").hover();
    await userEvent.keyboard("m");
    expect(core.calls("setNotificationMuted").length).toBe(2);

    // its panel offers Unmute while it is muted
    core.answer("item", itemDetail({ title: "Load themes", number: 1 }));
    core.answer("notificationOf", { ...update(1, "Load themes"), muted: true, done: true });
    await row("Load themes").click();
    await page.getByRole("button", { name: "Unmute notifications" }).click();
    expect(core.calls("setSubjectMuted").at(-1)).toEqual([{ item: "ACME-1" }, false]);
  });
});
