// The inbox's Updates at component level, against the fake core: a new
// notification arrives unread, and the person marks it read, unread and
// done, with the buttons and the keys, and marks every update read.
import { afterEach, describe, expect, test, vi } from "vitest";
import { page, userEvent } from "vitest/browser";
import type { Notice, Notification } from "../api/types";
import { core } from "../testing/core";
import { cleanup, render } from "../testing/render";
import { Inbox } from "./Inbox";

vi.mock("../api/client", async (real) => (await import("../testing/core")).fakeClient(await real()));

afterEach(() => {
  cleanup();
  core.reset();
});

const update = (id: number, title: string, extra: Partial<Notification> = {}): Notification => ({
  id,
  subject: { kind: "item", item: { id: `w_${id}`, key: `PIN-${id}`, title, status: "open" } },
  repo: "acme/web",
  project: null,
  reason: "item_commented",
  waiting: false,
  actor: { kind: "agent", name: "claude-code", session: null },
  summary: "The spec is ready.",
  unread: 1,
  read: false,
  done: false,
  muted: false,
  last_event: 10 + id,
  updated_at: new Date().toISOString(),
  ...extra,
});

/** The inbox, with no review waiting and the updates given. */
async function open(updates: Notification[]) {
  core.answer("listReviews", { reviews: [], total: 0 });
  core.answer("plugins", { plugins: [] });
  core.answer("sessions", { sessions: [], total: 0 });
  core.answer("notifications", {
    notifications: updates,
    total: updates.length,
    counts: { waiting: 0, unread: updates.filter((u) => !u.read).length },
    up_to: 40,
  });
  for (const write of ["setNotificationRead", "setNotificationDone", "readNotifications"] as const) {
    core.answer(write, undefined);
  }
  await render(<Inbox />);
  if (updates.length) await expect.element(page.getByTestId("update-row").first()).toBeVisible();
}

const row = (title: string) => page.getByTestId("update-row").filter({ hasText: title });
/** A notice that carries a notification, as the core sends it. */
const changed = (notification: Notification, unread: number) =>
  core.notice({
    event_id: notification.last_event,
    kind: "notification_changed",
    notification,
    counts: { waiting: 0, unread },
  } as unknown as Notice);

describe("the inbox's updates", () => {
  test("an agent's new comment appears first, unread, with who commented", async () => {
    await open([update(1, "Load themes", { read: true, unread: 0 })]);
    await expect.element(row("Load themes")).toHaveClass(/is-read/);
    core.notice({
      event_id: 50,
      kind: "item_changed",
      notification: update(2, "Fix the login redirect", { last_event: 50 }),
      counts: { waiting: 0, unread: 1 },
    } as unknown as Notice);
    // first, as the newest
    await vi.waitFor(() =>
      expect(page.getByTestId("update-row").elements()[0]?.textContent).toMatch(/^Fix the login redirect/),
    );
    await expect.element(row("Fix the login redirect")).toHaveClass(/is-unread/);
    await expect.element(row("Fix the login redirect")).toMatchTextContent("claude-code commented");
    await expect.element(page.getByText("1 unread")).toBeVisible();
  });

  test("the buttons mark an update read, unread and done", async () => {
    await open([update(1, "Load themes")]);
    await page.getByRole("button", { name: "Mark as read: Load themes" }).click();
    expect(core.calls("setNotificationRead").at(-1)).toEqual([1, true]);
    changed(update(1, "Load themes", { read: true, unread: 0 }), 0);
    await expect.element(row("Load themes")).toHaveClass(/is-read/);

    await page.getByRole("button", { name: "Mark as unread: Load themes" }).click();
    expect(core.calls("setNotificationRead").at(-1)).toEqual([1, false]);

    await page.getByRole("button", { name: "Mark as done: Load themes" }).click();
    expect(core.calls("setNotificationDone").at(-1)).toEqual([1, true]);
    changed(update(1, "Load themes", { done: true }), 0);
    await expect.element(row("Load themes")).not.toBeInTheDocument();
  });

  test("Mark all as read covers what the list showed", async () => {
    await open([update(1, "Load themes"), update(2, "Fix the login redirect")]);
    await page.getByRole("button", { name: "Mark all as read" }).click();
    expect(core.calls("readNotifications").at(-1)).toEqual(["all", 40]);
  });

  test("U, E and Shift-U act on the focused update", async () => {
    await open([update(1, "Load themes")]);
    await row("Load themes").hover();
    await expect.element(row("Load themes")).toHaveClass(/is-focused/);
    await userEvent.keyboard("u");
    expect(core.calls("setNotificationRead").at(-1)).toEqual([1, true]);
    await userEvent.keyboard("e");
    expect(core.calls("setNotificationDone").at(-1)).toEqual([1, true]);
    await userEvent.keyboard("{Shift>}U{/Shift}");
    expect(core.calls("readNotifications").at(-1)).toEqual(["all", 40]);
  });
});
