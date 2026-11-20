// A menu under its button, at component level: its keys.
import { useEffect, useState } from "react";
import { afterEach, expect, test } from "vitest";
import { page, userEvent } from "vitest/browser";
import { cleanup, render } from "../testing/render";
import { PopoverMenu } from "./PopoverMenu";

afterEach(cleanup);

/** A page that draws again on every key press, from a key listener added
 *  before the menu's: React then draws between the listeners of one key
 *  press, and gives the menu new props. */
function Page() {
  const [anchor, setAnchor] = useState<HTMLElement | null>(null);
  const [, setPressed] = useState(0);
  useEffect(() => {
    const onKey = () => setPressed((n) => n + 1);
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, []);
  return (
    <>
      <button type="button" onClick={(event) => setAnchor(event.currentTarget)}>
        Actions
      </button>
      {anchor ? (
        <PopoverMenu
          anchor={anchor}
          label="Actions"
          entries={[
            { label: "Rename", onPick: () => {} },
            { label: "Delete", onPick: () => {} },
          ]}
          onClose={() => setAnchor(null)}
        />
      ) : null}
    </>
  );
}

const menu = () => page.getByRole("menu", { name: "Actions" });

test("Esc closes the menu when the page draws again during the key press", async () => {
  await render(<Page />, { bare: true });
  await userEvent.click(page.getByRole("button", { name: "Actions" }));
  await expect.element(menu()).toBeVisible();
  await userEvent.keyboard("{Escape}");
  await expect.element(menu()).not.toBeInTheDocument();
});
