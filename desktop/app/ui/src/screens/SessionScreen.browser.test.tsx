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
