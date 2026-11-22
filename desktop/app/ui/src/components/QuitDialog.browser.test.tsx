// The dialog that asks what to do with the running agents when Pinrail
// quits.
import { afterEach, expect, test, vi } from "vitest";
import { page, userEvent } from "vitest/browser";
import { cleanup, render } from "../testing/render";
import { QuitDialog, type RunningAgent } from "./QuitDialog";

afterEach(() => cleanup());

const running: RunningAgent[] = [
  { agent: "claude-code", title: "Fix the login redirect", folder: "/work/api" },
  { agent: "codex", title: null, folder: "/work/app/" },
];

async function open(agents = running) {
  const onAnswer = vi.fn<(stop: boolean, remember: boolean) => Promise<void>>(async () => {});
  const onClose = vi.fn();
  await render(<QuitDialog running={agents} onAnswer={onAnswer} onClose={onClose} />);
  return { onAnswer, onClose };
}

test("lists the running agents, and Quit leaves them running", async () => {
  const { onAnswer } = await open();
  await expect
    .element(page.getByText("2 agents are running. They keep running after Pinrail quits", { exact: false }))
    .toBeVisible();
  const list = page.getByRole("list", { name: "Running agents" });
  await expect.element(list.getByText("Fix the login redirect")).toBeVisible();
  await expect.element(list.getByText("api", { exact: true })).toBeVisible();
  await expect.element(list.getByText("Codex session")).toBeVisible();
  await expect.element(list.getByText("app", { exact: true })).toBeVisible();
  await expect.element(page.getByRole("button", { name: "Quit", exact: true })).toHaveFocus();
  await userEvent.keyboard("{Enter}");
  expect(onAnswer).toHaveBeenCalledWith(false, false);
});

test("Stop agents and quit stops them, and Do not ask again remembers it", async () => {
  const { onAnswer } = await open(running.slice(0, 1));
  await expect
    .element(page.getByText("1 agent is running. It keeps running after Pinrail quits", { exact: false }))
    .toBeVisible();
  await userEvent.click(page.getByRole("checkbox", { name: "Do not ask again" }));
  await userEvent.click(page.getByRole("button", { name: "Stop agents and quit" }));
  expect(onAnswer).toHaveBeenCalledWith(true, true);
});

test("Cancel and Esc keep Pinrail open", async () => {
  const { onAnswer, onClose } = await open();
  await userEvent.click(page.getByRole("button", { name: "Cancel" }));
  await userEvent.keyboard("{Escape}");
  expect(onClose).toHaveBeenCalledTimes(2);
  expect(onAnswer).not.toHaveBeenCalled();
});

test("says why quitting failed and lets the person try again", async () => {
  const onAnswer = vi.fn(async () => {
    throw new Error("The terminal host is not connected.");
  });
  await render(<QuitDialog running={running} onAnswer={onAnswer} onClose={() => {}} />);
  await userEvent.click(page.getByRole("button", { name: "Stop agents and quit" }));
  await expect.element(page.getByText("The terminal host is not connected.")).toBeVisible();
  await expect.element(page.getByRole("button", { name: "Stop agents and quit" })).toBeEnabled();
});
