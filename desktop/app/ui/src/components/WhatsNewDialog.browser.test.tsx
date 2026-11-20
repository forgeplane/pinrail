// The What's new dialog's keys, at component level.
import { useEffect, useState } from "react";
import { afterEach, expect, test } from "vitest";
import { page, userEvent } from "vitest/browser";
import { cleanup, render } from "../testing/render";
import { WhatsNewDialog } from "./WhatsNewDialog";

afterEach(cleanup);

const releases = [
  {
    version: "1.2.0",
    cards: [
      { title: "First", text: "The first card." },
      { title: "Second", text: "The second card." },
    ],
  },
];

/** A page that draws again on every key press, from a key listener added
 *  before the dialog's, as the app's layout does when its state changes:
 *  React then draws between the two listeners of one key press. */
function Page() {
  const [open, setOpen] = useState(false);
  const [, setPressed] = useState(0);
  useEffect(() => {
    const onKey = () => setPressed((n) => n + 1);
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);
  return (
    <>
      <button type="button" onClick={() => setOpen(true)}>
        Open
      </button>
      {open ? <WhatsNewDialog releases={releases} onAction={() => {}} onClose={() => setOpen(false)} /> : null}
    </>
  );
}

const dialog = () => page.getByRole("dialog", { name: "What's new in Pinrail" });

test("Esc closes the dialog when the page draws again during the key press", async () => {
  await render(<Page />, { bare: true });
  await userEvent.click(page.getByRole("button", { name: "Open" }));
  await expect.element(dialog()).toBeVisible();
  await userEvent.keyboard("{Escape}");
  await expect.element(dialog()).not.toBeInTheDocument();
});

test("the arrow keys change the card when the page draws again during the key press", async () => {
  await render(<Page />, { bare: true });
  await userEvent.click(page.getByRole("button", { name: "Open" }));
  await expect.element(dialog().getByRole("heading", { name: "First" })).toBeVisible();
  await userEvent.keyboard("{ArrowRight}");
  await expect.element(dialog().getByRole("heading", { name: "Second" })).toBeVisible();
  await userEvent.keyboard("{ArrowLeft}");
  await expect.element(dialog().getByRole("heading", { name: "First" })).toBeVisible();
});
