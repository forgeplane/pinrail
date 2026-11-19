// The sidebar's main links at component level: which one marks the page
// that shows.
import { afterEach, describe, expect, test, vi } from "vitest";
import { page } from "vitest/browser";
import { core } from "../testing/core";
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
