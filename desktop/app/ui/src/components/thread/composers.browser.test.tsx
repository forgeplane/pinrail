// The message box under a session's thread, against the fake core: files
// pasted, dropped and removed, and sent as paths after the text.
import { afterEach, expect, test, vi } from "vitest";
import { page, userEvent } from "vitest/browser";
import { core } from "../../testing/core";
import { cleanup, render } from "../../testing/render";
import { MessageComposer } from "./composers";

vi.mock("../../api/client", async (real) => (await import("../../testing/core")).fakeClient(await real()));

afterEach(() => {
  cleanup();
  core.reset();
  sessionStorage.clear();
});

const session = { agent: "claude-code", id: "ses_1" };

/** The box, against a core that saves each file in the session's folder. */
async function open() {
  core.answer("items", { items: [] });
  core.answer("saveSessionFile", (_agent: unknown, _id: unknown, file: unknown) => {
    const name = (file as File).name;
    return { name, path: `/data/sessions/claude-code/ses_1/files/${name}` };
  });
  core.answer("sendMessage", { id: 1 });
  await render(<MessageComposer session={session} repo={null} typed onSent={() => {}} />);
}

const box = () => page.getByRole("textbox", { name: "Message to the agent" });
const chip = (name: string) =>
  page.getByRole("list", { name: "Attached files" }).getByRole("listitem").filter({ hasText: name });

/** A one-pixel PNG, as a screenshot pasted from the clipboard. */
async function screenshot(): Promise<File> {
  const canvas = document.createElement("canvas");
  canvas.width = canvas.height = 1;
  const blob = await new Promise<Blob>((done) => canvas.toBlob((b) => done(b!), "image/png"));
  return new File([blob], "screen shot.png", { type: "image/png" });
}

test("a pasted image and a dropped file show as chips and go after the text as paths", async () => {
  await open();
  const field = box().element();

  const pasted = new DataTransfer();
  pasted.items.add(await screenshot());
  field.dispatchEvent(new ClipboardEvent("paste", { clipboardData: pasted, bubbles: true, cancelable: true }));
  await expect.element(chip("screen shot.png")).toBeVisible();
  // an image shows its picture
  expect(chip("screen shot.png").element().querySelector("img")).not.toBeNull();

  const dropped = new DataTransfer();
  dropped.items.add(new File(["# Notes"], "notes.md", { type: "text/markdown" }));
  field.dispatchEvent(new DragEvent("drop", { dataTransfer: dropped, bubbles: true, cancelable: true }));
  await expect.element(chip("notes.md")).toBeVisible();
  expect(core.calls("saveSessionFile").map(([agent, id]) => [agent, id])).toEqual([
    ["claude-code", "ses_1"],
    ["claude-code", "ses_1"],
  ]);

  await page.getByRole("button", { name: "Remove notes.md" }).click();
  await expect.element(chip("notes.md")).not.toBeInTheDocument();

  await box().click();
  await userEvent.keyboard("What is wrong here?{Enter}");
  await expect.poll(() => core.calls("sendMessage").length).toBe(1);
  const [, , text] = core.calls("sendMessage")[0];
  expect(text).toBe("What is wrong here?\n\n'/data/sessions/claude-code/ses_1/files/screen shot.png'");
  await expect.element(chip("screen shot.png")).not.toBeInTheDocument();
});

test("files alone can be sent, and a file still saving holds the send", async () => {
  await open();
  let finish: (saved: { name: string; path: string }) => void = () => {};
  core.answer("saveSessionFile", () => new Promise((done) => (finish = done)));
  const picker = document.querySelector<HTMLInputElement>('.thread-composer input[type="file"]')!;
  await userEvent.upload(picker, new File(["log"], "build.log", { type: "text/plain" }));
  await expect.element(chip("build.log")).toBeVisible();
  await expect.element(page.getByTestId("send-message")).toBeDisabled();

  finish({ name: "build.log", path: "/data/sessions/claude-code/ses_1/files/build.log" });
  await expect.element(page.getByTestId("send-message")).toBeEnabled();
  await page.getByTestId("send-message").click();
  await expect.poll(() => core.calls("sendMessage").length).toBe(1);
  expect(core.calls("sendMessage")[0][2]).toBe("/data/sessions/claude-code/ses_1/files/build.log");
});
