// Following the app's updates, with Tauri's calls and events replaced: the
// status read once, then each one the app sends.
import { afterEach, expect, test, vi } from "vitest";
import { page, userEvent } from "vitest/browser";
import { cleanup, render } from "../testing/render";
import { useUpdates, type UpdateStatus } from "./updates";

const tauri = vi.hoisted(() => ({
  answers: {} as Record<string, unknown>,
  invoked: [] as string[],
  listener: null as ((event: { payload: unknown }) => void) | null,
}));
vi.mock("../api/client", async (real) => ({ ...(await real()), inTauri: () => true }));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: async (command: string) => {
    tauri.invoked.push(command);
    return tauri.answers[command];
  },
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: async (_event: string, handler: (event: { payload: unknown }) => void) => {
    tauri.listener = handler;
    return () => (tauri.listener = null);
  },
}));

afterEach(() => {
  cleanup();
  tauri.invoked = [];
  tauri.listener = null;
});

function Probe() {
  const { status, check } = useUpdates();
  return (
    <>
      <output>{status.state}</output>
      <button type="button" onClick={() => void check()}>
        Check
      </button>
    </>
  );
}

const send = (status: UpdateStatus) => tauri.listener?.({ payload: status });

test("reads the status, then follows each one the app sends", async () => {
  tauri.answers.update_status = { state: "idle" };
  await render(<Probe />, { bare: true });
  await expect.element(page.getByText("idle")).toBeVisible();
  send({ state: "downloading", version: "0.7.0", percent: 40 });
  await expect.element(page.getByText("downloading")).toBeVisible();
  send({ state: "ready", version: "0.7.0", notes: null });
  await expect.element(page.getByText("ready")).toBeVisible();
});

test("Check asks the app, and shows what it answers", async () => {
  tauri.answers.update_status = { state: "idle" };
  tauri.answers.check_for_updates = { state: "up_to_date", checked_at: "2026-11-19T09:00:00Z" };
  await render(<Probe />, { bare: true });
  await userEvent.click(page.getByRole("button", { name: "Check" }));
  await expect.element(page.getByText("up_to_date")).toBeVisible();
  expect(tauri.invoked).toContain("check_for_updates");
});

test("stops listening when the component goes", async () => {
  tauri.answers.update_status = { state: "idle" };
  await render(<Probe />, { bare: true });
  await expect.poll(() => tauri.listener).not.toBeNull();
  cleanup();
  expect(tauri.listener).toBeNull();
});
