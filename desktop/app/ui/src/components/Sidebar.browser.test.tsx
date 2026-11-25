// The sidebar's main links at component level: which one marks the page
// that shows.
import { afterEach, describe, expect, test, vi } from "vitest";
import { page } from "vitest/browser";
import type { Notice } from "../api/types";
import { core } from "../testing/core";
import { session } from "../testing/items";
import { cleanup, render } from "../testing/render";
import { Sidebar, useSidebarWidth } from "./Sidebar";

vi.mock("../api/client", async (real) => (await import("../testing/core")).fakeClient(await real()));

afterEach(() => {
  cleanup();
  core.reset();
});

function Shown() {
  const width = useSidebarWidth();
  return (
    <Sidebar
      width={width}
      toggle={null}
      shown={null}
      openItems={0}
      itemsInProgress={0}
      onSearch={() => {}}
      onSettings={() => {}}
      onFeedback={() => {}}
    />
  );
}

// a link's name starts with its label, and goes on with its count or keys
const link = (label: string) =>
  page.getByRole("navigation", { name: "Main" }).getByRole("link", { name: new RegExp(`^${label}\\b`) });

describe("the sidebar's links", () => {
  test("Sessions marks the Sessions page", async () => {
    await render(<Shown />, { route: "/sessions" });
    await expect.element(link("Sessions")).toHaveClass(/is-active/);
  });

  test("with a session open, Sessions is not marked: the session's own row is", async () => {
    await render(<Shown />, { route: "/sessions/claude-code/s-1" });
    await expect.element(link("Sessions")).toBeVisible();
    await expect.element(link("Sessions")).not.toHaveClass(/is-active/);
  });
});

describe("the inbox's counts", () => {
  test("the unread updates count rises with a notice and falls when one is marked read", async () => {
    core.answer("listReviews", { reviews: [], total: 0 });
    core.answer("plugins", { plugins: [] });
    core.answer("notificationCounts", { waiting: 0, unread: 2 });
    await render(<Shown />, { route: "/sessions" });
    const unread = page.getByTestId("inbox-unread");
    await expect.element(unread).toHaveTextContent("2");
    // nothing waits: the waiting count gives way to the unread one
    await expect.element(page.getByTestId("inbox-count")).not.toBeInTheDocument();

    core.notice({ event_id: 5, kind: "item_changed", counts: { waiting: 0, unread: 3 } } as unknown as Notice);
    await expect.element(unread).toHaveTextContent("3");
    // the core's notice after Mark as read
    core.notice({ event_id: 6, kind: "notification_changed", counts: { waiting: 0, unread: 2 } } as unknown as Notice);
    await expect.element(unread).toHaveTextContent("2");
    core.notice({ event_id: 7, kind: "notification_changed", counts: { waiting: 0, unread: 0 } } as unknown as Notice);
    await expect.element(unread).not.toBeInTheDocument();
  });
});

describe("the sidebar's sessions", () => {
  test("a session started from Pinrail shows as its run starts", async () => {
    core.answer("sessions", { sessions: [], total: 0 });
    await render(<Shown />, { route: "/sessions" });
    await expect.element(page.getByText("No sessions yet")).toBeVisible();
    core.answer("sessions", { sessions: [session({ id: "ses_1", title: "Fix the login" })], total: 1 });
    core.notice({ event_id: 5, kind: "run_started", run: { id: "run_1" } } as unknown as Notice);
    await expect.element(page.getByTestId("sidebar-session").filter({ hasText: "Fix the login" })).toBeVisible();
  });
});
