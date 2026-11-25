// The line above the message box that says what the agent waits for.
import { afterEach, describe, expect, test, vi } from "vitest";
import { page } from "vitest/browser";
import { cleanup, render } from "../../testing/render";
import { WaitingLine } from "./runLines";

afterEach(() => cleanup());

const permission = { kind: "permission", tool: "Bash", summary: "cargo test" } as Parameters<
  typeof WaitingLine
>[0]["wait"];

describe("the waiting line", () => {
  test("a click anywhere on it opens the terminal Pinrail runs the agent in", async () => {
    const onOpen = vi.fn();
    await render(<WaitingLine name="Claude Code" wait={permission} onOpen={onOpen} />);
    await page.getByText("Claude Code needs permission").click();
    expect(onOpen).toHaveBeenCalledTimes(1);
    await page.getByRole("button", { name: "Open the terminal" }).click();
    expect(onOpen).toHaveBeenCalledTimes(2);
  });
});
