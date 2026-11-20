// A session's thread at component level, against the fake core: its header
// and renaming in place, its timeline, and what its notices change.
import { afterEach, describe, expect, test, vi } from "vitest";
import { page, userEvent } from "vitest/browser";
import type { Notice, SessionStatus } from "../api/types";
import { core } from "../testing/core";
import { session } from "../testing/items";
import { cleanup, render } from "../testing/render";
import { Thread } from "./Thread";

vi.mock("../api/client", async (real) => (await import("../testing/core")).fakeClient(await real()));

afterEach(() => {
  cleanup();
  core.reset();
});

const step = (id: number, text: string, kind: SessionStatus["kind"] = "step"): SessionStatus => ({
  id,
  text,
  kind,
  at: `2026-11-19T09:0${id}:00Z`,
});

/** The thread of session `s-1`, with these statuses, as the core keeps it. */
async function open(statuses: SessionStatus[], fields: Parameters<typeof session>[0] = { id: "s-1" }) {
  const shown = { session: session(fields), items: [], statuses, messages: [] };
  core.answer("session", () => shown);
  core.answer("items", { items: [] });
  core.answer("renameSession", (_agent: unknown, _id: unknown, name: unknown) => {
    shown.session = { ...shown.session, name: name as string | null };
  });
  await render(<Thread session={{ agent: "claude-code", id: "s-1" }} />, { route: "/sessions/claude-code/s-1" });
  await expect.element(heading()).toBeVisible();
  return shown;
}

const heading = () => page.getByTestId("thread").getByRole("heading", { level: 1 });

describe("the thread's header", () => {
  test("names the session by its stage, with its generated name beside it", async () => {
    await open([step(1, "Fix the login", "start")], { id: "s-1", title: "Fix the login" });
    await expect.element(heading()).toHaveTextContent("Fix the login");
    await expect.element(page.getByTestId("thread").getByText("Happy Hydrogen")).toBeVisible();
  });

  test("without a stage, by its generated name", async () => {
    await open([]);
    await expect.element(heading()).toHaveTextContent("Happy Hydrogen");
  });

  test("a click on the name renames it in place, and Enter saves it", async () => {
    await open([step(1, "Fix the login", "start")], { id: "s-1", title: "Fix the login" });
    await userEvent.click(heading().getByRole("button"));
    const field = heading().getByRole("textbox");
    await expect.element(field).toHaveFocus();
    await userEvent.fill(field, "Auth fixes");
    await userEvent.keyboard("{Enter}");
    await expect.poll(() => core.calls("renameSession")).toEqual([["claude-code", "s-1", "Auth fixes"]]);
    await expect.element(heading()).toHaveTextContent("Auth fixes");
  });

  test("Esc leaves the name as it was", async () => {
    await open([], { id: "s-1" });
    await userEvent.click(heading().getByRole("button"));
    await userEvent.fill(heading().getByRole("textbox"), "Never mind");
    await userEvent.keyboard("{Escape}");
    await expect.element(heading()).toHaveTextContent("Happy Hydrogen");
    expect(core.calls("renameSession")).toEqual([]);
  });
});

describe("the timeline", () => {
  test("shows the statuses in order", async () => {
    await open([step(1, "Fix the login", "start"), step(2, "Reading the code"), step(3, "Writing the test")], {
      id: "s-1",
      title: "Fix the login",
    });
    const thread = page.getByTestId("thread");
    await expect.element(thread.getByText("Writing the test")).toBeVisible();
    const text = (await thread.element()).textContent ?? "";
    expect(text.indexOf("Reading the code")).toBeLessThan(text.indexOf("Writing the test"));
  });

  test("a status the agent reports shows without a reload of the page", async () => {
    const shown = await open([step(1, "Fix the login", "start")], { id: "s-1", title: "Fix the login" });
    shown.statuses = [...shown.statuses, step(2, "Running the tests")];
    core.notice({
      kind: "session_status",
      session: { agent: "claude-code", id: "s-1", status: step(2, "Running the tests") },
    } as unknown as Notice);
    await expect.element(page.getByTestId("thread").getByText("Running the tests")).toBeVisible();
  });

  test("a status of another session changes nothing here", async () => {
    await open([step(1, "Fix the login", "start")], { id: "s-1", title: "Fix the login" });
    const loads = core.calls("session").length;
    core.notice({
      kind: "session_status",
      session: { agent: "claude-code", id: "s-other", status: step(9, "Elsewhere") },
    } as unknown as Notice);
    await new Promise((resolve) => setTimeout(resolve, 300));
    expect(core.calls("session").length).toBe(loads);
  });
});

describe("scrolling", () => {
  /** A thread of many notes, in a box of a fixed height, with the app's
   *  styles, so that its timeline scrolls. */
  async function tall() {
    await import("../theme.css");
    const notes = Array.from({ length: 30 }, (_, i) =>
      step(i + 1, `Note ${i + 1}\n\n${"Some words. ".repeat(20)}`, "note"),
    );
    const shown = { session: session({ id: "s-1", title: "Long work" }), items: [], statuses: notes, messages: [] };
    core.answer("session", () => shown);
    core.answer("items", { items: [] });
    await render(
      <div style={{ height: 500, display: "flex", flexDirection: "column" }}>
        <Thread session={{ agent: "claude-code", id: "s-1" }} />
      </div>,
      { route: "/sessions/claude-code/s-1" },
    );
    await expect.element(page.getByTestId("thread").getByText("Note 30")).toBeInTheDocument();
    const timeline = document.querySelector<HTMLElement>(".thread-items")!;
    await expect.poll(() => timeline.scrollHeight).toBeGreaterThan(timeline.clientHeight * 2);
    const add = (n: number) => {
      shown.statuses = [...shown.statuses, step(n, `Note ${n}`, "note")];
      core.notice({
        kind: "session_status",
        session: { agent: "claude-code", id: "s-1", status: step(n, `Note ${n}`, "note") },
      } as unknown as Notice);
    };
    return { timeline, add };
  }
  const atEnd = (el: HTMLElement) => el.scrollHeight - el.scrollTop - el.clientHeight < 4;
  const toEnd = () => page.getByRole("button", { name: "Scroll to the newest message" });

  test("the thread opens at its newest entry, and stays there as entries arrive", async () => {
    const { timeline, add } = await tall();
    await expect.poll(() => atEnd(timeline)).toBe(true);
    await expect.element(toEnd()).not.toBeInTheDocument();
    add(31);
    await expect.element(page.getByTestId("thread").getByText("Note 31")).toBeInTheDocument();
    await expect.poll(() => atEnd(timeline)).toBe(true);
  });

  test("scrolled up, it keeps its place as entries arrive, and the button takes it to the end", async () => {
    const { timeline, add } = await tall();
    await expect.poll(() => atEnd(timeline)).toBe(true);
    timeline.scrollTop = 200;
    await expect.element(toEnd()).toBeVisible();
    add(31);
    await expect.element(page.getByTestId("thread").getByText("Note 31")).toBeInTheDocument();
    expect(timeline.scrollTop).toBe(200);
    await userEvent.click(toEnd());
    await expect.poll(() => atEnd(timeline)).toBe(true);
  });
});

describe("the session's details", () => {
  const details = () => page.getByTestId("thread").getByRole("definition");

  test("show the agent's own id for the session as Agent session", async () => {
    await open([], { id: "ses_01ABC", agent_session: "3f2a9c1e-5b7d" });
    await expect.element(page.getByTestId("thread").getByText("Agent session")).toBeVisible();
    await expect.element(details().filter({ hasText: "3f2a9c1e-5b7d" })).toBeVisible();
    await expect.element(page.getByTestId("thread").getByText("ses_01ABC")).not.toBeInTheDocument();
  });

  test("say the agent's id is not known yet before the agent reports it", async () => {
    await open([], { id: "ses_01ABC", agent_session: null });
    await expect.element(page.getByTestId("thread").getByText("Agent session")).toBeVisible();
    await expect.element(details().filter({ hasText: "Not known yet" })).toBeVisible();
  });
});
