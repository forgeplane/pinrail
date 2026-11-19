// What macOS says of the app's notifications, with Tauri's calls replaced:
// asked when the screen opens and again when the window comes back.
import { afterEach, expect, test, vi } from "vitest";
import { page } from "vitest/browser";
import { cleanup, render } from "../testing/render";
import { useNotificationStatus, type NotificationStatus } from "./notifications";

const tauri = vi.hoisted(() => ({ status: null as unknown, asked: 0 }));
vi.mock("../api/client", async (real) => ({ ...(await real()), inTauri: () => true }));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: async (command: string) => {
    if (command !== "notification_status") return null;
    tauri.asked += 1;
    return tauri.status;
  },
}));

afterEach(() => {
  cleanup();
  tauri.asked = 0;
});

const allowed: NotificationStatus = {
  authorization: "authorized",
  alert_style: "banner",
  alerts: true,
  sound: true,
  badge: true,
  shows: true,
};

function Probe({ open }: { open: boolean }) {
  const { system } = useNotificationStatus(open);
  return <output>{system.known ? (system.status?.authorization ?? "no status") : "unknown"}</output>;
}

test("asks once the screen is open, and again when the window comes back", async () => {
  tauri.status = allowed;
  await render(<Probe open />, { bare: true });
  await expect.element(page.getByText("authorized")).toBeVisible();
  tauri.status = { ...allowed, authorization: "denied" };
  window.dispatchEvent(new Event("focus"));
  await expect.element(page.getByText("denied")).toBeVisible();
  expect(tauri.asked).toBe(2);
});

test("asks nothing while the screen is closed", async () => {
  await render(<Probe open={false} />, { bare: true });
  await expect.element(page.getByText("unknown")).toBeVisible();
  window.dispatchEvent(new Event("focus"));
  expect(tauri.asked).toBe(0);
});
