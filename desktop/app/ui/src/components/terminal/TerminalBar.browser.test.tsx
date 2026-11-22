// The bar below a session's terminal: a dot in the tone of the terminal's
// state, what that state is, its size and the key to the summary.
import { afterEach, expect, test } from "vitest";
import { page } from "vitest/browser";
import { cleanup, render } from "../../testing/render";
import { TerminalBar } from "./TerminalPane";
import type { TerminalState } from "./terminalPool";

afterEach(() => cleanup());

const state = (over: Partial<TerminalState>) =>
  ({
    stream: "open",
    role: "observer",
    pty: { cols: 97, rows: 36 },
    fit: { cols: 97, rows: 36 },
    ...over,
  }) as TerminalState;

test.each([
  ["viewing", state({}), "Viewing · click or type to take control", "is-idle"],
  ["typing", state({ role: "controller" }), "You are typing", "is-ok"],
  ["reconnecting", state({ stream: "reconnecting" }), "Reconnecting to the terminal…", "is-waiting"],
  ["disconnected", state({ stream: "closed" }), "Disconnected from the terminal.", "is-danger"],
])("while %s, the dot and the text say so", async (_, given, text, tone) => {
  await render(<TerminalBar state={given} failed={null} />);
  const status = page.getByRole("status");
  await expect.element(status).toHaveTextContent(text);
  expect(status.element().querySelector(".terminal-dot")?.classList.contains(tone)).toBe(true);
});

test("keeps the size, and shows the key to the summary as a key cap", async () => {
  await render(<TerminalBar state={state({})} failed={null} />);
  await expect.element(page.getByText("97 × 36")).toBeVisible();
  await expect.element(page.getByText("Ctrl+`", { exact: true })).toHaveClass("key-cap");
});
