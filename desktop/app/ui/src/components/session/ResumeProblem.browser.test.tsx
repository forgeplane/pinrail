// Why a resume did not happen, and the folder to choose when the session's
// own is gone.
import { afterEach, expect, test, vi } from "vitest";
import { page, userEvent } from "vitest/browser";
import { cleanup, render } from "../../testing/render";
import { ResumeProblem } from "./ResumeProblem";

afterEach(() => cleanup());

const gone = "The folder /work/app no longer exists, so the session cannot be resumed here.";

test("offers another folder when the session's folder is gone", async () => {
  const onChooseFolder = vi.fn();
  await render(<ResumeProblem message={gone} onChooseFolder={onChooseFolder} />);
  await expect.element(page.getByText(gone)).toBeVisible();
  await userEvent.click(page.getByRole("button", { name: "Choose another folder…" }));
  expect(onChooseFolder).toHaveBeenCalledOnce();
});

test("says why alone when no folder can be chosen", async () => {
  await render(<ResumeProblem message="This session is already running in another terminal." />);
  await expect.element(page.getByRole("alert")).toBeVisible();
  await expect.element(page.getByRole("button")).not.toBeInTheDocument();
});
