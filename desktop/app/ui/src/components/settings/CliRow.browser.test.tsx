// Settings › Data's row for the CLI, against a stand-in for the app's
// answer to cli_status.
import { afterEach, expect, test, vi } from "vitest";
import { page } from "vitest/browser";
import { cleanup, render } from "../../testing/render";
import { CliRow, type CliStatus } from "./CliRow";

const native = vi.hoisted(() => ({ status: null as unknown }));
vi.mock("../../api/client", async (real) => ({ ...(await real()), inTauri: () => true }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: async () => native.status }));

afterEach(() => cleanup());

const windows = (fields: Partial<CliStatus>): CliStatus => ({
  mode: "copy",
  bundled: String.raw`C:\Users\ann\AppData\Local\Pinrail\pinrail-cli.exe`,
  link: String.raw`C:\Users\ann\.local\bin\pinrail.exe`,
  installed: true,
  outdated: false,
  occupied_by: null,
  runs: null,
  dir_on_path: false,
  warning: null,
  profile: null,
  profile_line: "$k = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey('Environment')",
  user_path: true,
  ...fields,
});

test("when Windows' PATH could not be changed, the row still gives the line to run", async () => {
  native.status = windows({ warning: "the PATH was not updated: access denied" });
  render(<CliRow open />);
  await expect.element(page.getByText(/access denied/)).toBeVisible();
  await expect.element(page.getByText(/CreateSubKey\('Environment'\)/)).toBeVisible();
  await expect.element(page.getByRole("button", { name: "Add it to your PATH" })).toBeVisible();
});
