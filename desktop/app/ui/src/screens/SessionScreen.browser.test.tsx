// A session's page at component level, against the fake core: an address
// that names the session by its agent's own id, as links from before
// Pinrail's ids do, is replaced by the one with Pinrail's id.
import { Route, Routes, useLocation } from "react-router";
import { afterEach, expect, test, vi } from "vitest";
import { page } from "vitest/browser";
import { core } from "../testing/core";
import { session } from "../testing/items";
import { cleanup, render } from "../testing/render";
import { SessionScreen } from "./Sessions";

vi.mock("../api/client", async (real) => (await import("../testing/core")).fakeClient(await real()));

afterEach(() => {
  cleanup();
  core.reset();
});

function Where() {
  const { pathname, search } = useLocation();
  return <output data-testid="where">{pathname + search}</output>;
}

/** The session page at `route`, with the session the core has. */
async function open(route: string) {
  const shown = session({ id: "ses_01ABC", agent_session: "3f2a9c1e-5b7d", title: "Fix the login" });
  core.answer("session", () => ({ session: shown, items: [], statuses: [], messages: [] }));
  core.answer("items", { items: [] });
  await render(
    <Routes>
      <Route
        path="/sessions/:agent/:id"
        element={
          <>
            <SessionScreen />
            <Where />
          </>
        }
      />
    </Routes>,
    { route },
  );
}

test("an address with the agent's own id is replaced by Pinrail's", async () => {
  await open("/sessions/claude-code/3f2a9c1e-5b7d?view=summary");
  await expect.element(page.getByTestId("where")).toHaveTextContent("/sessions/claude-code/ses_01ABC?view=summary");
  await expect
    .element(page.getByTestId("thread").getByRole("heading", { level: 1 }))
    .toHaveTextContent("Fix the login");
  expect(core.calls("session").at(-1)).toEqual(["claude-code", "ses_01ABC"]);
});

test("an address with Pinrail's id stays as it is", async () => {
  await open("/sessions/claude-code/ses_01ABC");
  await expect
    .element(page.getByTestId("thread").getByRole("heading", { level: 1 }))
    .toHaveTextContent("Fix the login");
  expect(new Set(core.calls("session").map((call) => call.join("/")))).toEqual(new Set(["claude-code/ses_01ABC"]));
  await expect.element(page.getByTestId("where")).toHaveTextContent("/sessions/claude-code/ses_01ABC");
});

test("the page follows the session's run to the new session it goes on in, without drawing it again", async () => {
  const sessions = {
    ses_01ABC: session({ id: "ses_01ABC", agent_session: "3f2a9c1e-5b7d", title: "Fix the login" }),
    ses_02DEF: session({ id: "ses_02DEF", agent_session: "8d1c0b7a-2e4f", title: "Fix the logout" }),
  };
  core.answer("session", (_agent: unknown, id: unknown) => ({
    session: sessions[id as keyof typeof sessions],
    items: [],
    statuses: [],
    messages: [],
  }));
  core.answer("items", { items: [] });
  await render(
    <Routes>
      <Route
        path="/sessions/:agent/:id"
        element={
          <>
            <SessionScreen />
            <Where />
          </>
        }
      />
    </Routes>,
    { route: "/sessions/claude-code/ses_01ABC?view=terminal" },
  );
  const heading = page.getByTestId("thread").getByRole("heading", { level: 1 });
  await expect.element(heading).toHaveTextContent("Fix the login");
  const before = page.getByTestId("thread").element();

  // another session's run moving changes nothing here
  core.notice({
    event_id: 8,
    kind: "run_bound",
    at: null,
    actor: null,
    subject: null,
    review_id: null,
    review: null,
    run: {
      run: "run_2",
      session: { agent: "claude-code", id: "ses_09XYZ", agent_session: "x" },
      previous: { agent: "claude-code", id: "ses_08UVW" },
    },
  });
  // /clear moves this one's run
  core.notice({
    event_id: 9,
    kind: "run_bound",
    at: null,
    actor: null,
    subject: null,
    review_id: null,
    review: null,
    run: {
      run: "run_1",
      session: { agent: "claude-code", id: "ses_02DEF", agent_session: "8d1c0b7a-2e4f" },
      previous: { agent: "claude-code", id: "ses_01ABC" },
    },
  });
  await expect.element(page.getByTestId("where")).toHaveTextContent("/sessions/claude-code/ses_02DEF?view=summary");
  await expect.element(heading).toHaveTextContent("Fix the logout");
  expect(page.getByTestId("thread").element()).toBe(before);
});

test("a name given while the thread loads again stays when that load answers without it", async () => {
  await open("/sessions/claude-code/ses_01ABC");
  const heading = page.getByTestId("thread").getByRole("heading", { level: 1 });
  await expect.element(heading).toHaveTextContent("Fix the login");
  // the loads the page makes as it opens are over
  const opened = core.calls("session").length;
  await new Promise((resolve) => setTimeout(resolve, 50));
  expect(core.calls("session").length).toBe(opened);

  // the thread loads again, and the core reads the session before the rename
  let answer!: (thread: unknown) => void;
  core.answer("session", () => new Promise((resolve) => (answer = resolve)));
  const about = { agent: "claude-code", id: "ses_01ABC" };
  core.notice({ event_id: 8, kind: "session_seen", review_id: null, review: null, session: about } as never);
  await expect.poll(() => core.calls("session").length).toBe(opened + 1);

  core.notice({
    event_id: 9,
    kind: "session_renamed",
    review_id: null,
    review: null,
    session: { ...about, name: "Deploy work" },
  } as never);
  await expect.element(heading).toHaveTextContent("Deploy work");

  // the load's answer arrives after the notice, on a later stage and unnamed
  const unnamed = session({ id: "ses_01ABC", agent_session: "3f2a9c1e-5b7d", title: "Fix the logout" });
  answer({ session: unnamed, items: [], statuses: [], messages: [] });
  await expect.element(page.getByTestId("thread").getByTitle("Fix the logout")).toBeInTheDocument();
  await expect.element(heading).toHaveTextContent("Deploy work");
});
