// A message or a prompt with attached files: badges in place of the paths.
import { afterEach, expect, test, vi } from "vitest";
import { page } from "vitest/browser";
import { core } from "../../testing/core";
import { cleanup, render } from "../../testing/render";
import { AttachedText } from "./AttachedText";

vi.mock("../../api/client", async (real) => (await import("../../testing/core")).fakeClient(await real()));

afterEach(() => {
  cleanup();
  core.reset();
});

const folder = "/data/sessions/claude-code/ses_1/files";

test("a prompt shows its files as a note's chips, opens an image in a pop-up, and has no paths", async () => {
  await render(
    <div data-testid="prompt">
      <AttachedText text={`[Image #1]What is wrong here?\n\n${folder}/shot.png\n${folder}/build.log`} />
    </div>,
  );
  // the words, the files' names and the agent's image: no path
  await expect.element(page.getByTestId("prompt")).toHaveTextContent("What is wrong here?shot.pngbuild.logImage");

  const image = page.getByRole("button", { name: "Open shot.png" });
  const src = image.element().querySelector("img")!.getAttribute("src")!;
  expect(src).toMatch(/\/api\/v1\/sessions\/claude-code\/ses_1\/files\/shot\.png$/);
  await image.click();
  const popup = page.getByRole("dialog", { name: "shot.png" });
  await expect.element(popup.getByRole("img", { name: "shot.png" })).toBeVisible();
  await popup.getByRole("button", { name: "Close" }).click();
  await expect.element(popup).not.toBeInTheDocument();

  // outside the app, a file is downloaded rather than shown in Finder
  await expect.element(page.getByRole("button", { name: "Save build.log" })).toBeVisible();
});
