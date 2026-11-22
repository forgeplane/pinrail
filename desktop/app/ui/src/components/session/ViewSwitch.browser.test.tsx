// The session's view switch: Ctrl+` switches between the summary and the
// terminal, so only their tooltips show it.
import { afterEach, expect, test } from "vitest";
import { page, userEvent } from "vitest/browser";
import { cleanup, render } from "../../testing/render";
import { ViewSwitch } from "./ViewSwitch";

afterEach(() => cleanup());

test("only Summary and Terminal show Ctrl+` in their tooltips", async () => {
  await render(<ViewSwitch view="summary" onView={() => {}} />);
  for (const [name, keys] of [
    ["Summary", "Ctrl`"],
    ["Terminal", "Ctrl`"],
    ["Split view", ""],
  ]) {
    await userEvent.hover(page.getByRole("tab", { name }));
    const tip = page.getByRole("tooltip");
    // the label, then the keys, if any
    await expect.element(tip).toHaveTextContent(name + keys);
    await userEvent.unhover(page.getByRole("tab", { name }));
    await expect.element(tip).not.toBeInTheDocument();
  }
});
