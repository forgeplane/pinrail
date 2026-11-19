import { beforeEach, describe, expect, test, vi } from "vitest";
import type { NotificationStatus } from "./notifications";

const where = vi.hoisted(() => ({ tauri: true, mac: true }));
vi.mock("../api/client", () => ({ inTauri: () => where.tauri }));
vi.mock("../lib/keys", () => ({
  get isMac() {
    return where.mac;
  },
}));

const { describeSystem } = await import("./notifications");

const allowed: NotificationStatus = {
  authorization: "authorized",
  alert_style: "banner",
  alerts: true,
  sound: true,
  badge: true,
  shows: true,
};
const known = (status: Partial<NotificationStatus> | null) => ({
  known: true,
  status: status && { ...allowed, ...status },
});

beforeEach(() => {
  where.tauri = true;
  where.mac = true;
});

describe("describeSystem", () => {
  test("outside the app, says the system's answer shows only in the app", () => {
    where.tauri = false;
    expect(describeSystem(known(null))).toBe("What macOS allows shows here in the app");
    where.mac = false;
    expect(describeSystem(known(null))).toBe("What your system allows shows here in the app");
  });

  test("waits while the system has not answered", () => {
    expect(describeSystem({ known: false, status: null })).toBe("…");
  });

  test("with no status, says why: a development build on macOS, the desktop elsewhere", () => {
    expect(describeSystem(known(null))).toMatch(/development build/);
    where.mac = false;
    expect(describeSystem(known(null))).toMatch(/does not report its settings/);
  });

  test("says what keeps notifications from showing", () => {
    expect(describeSystem(known({ authorization: "denied" }))).toBe("Not allowed in System Settings");
    expect(describeSystem(known({ authorization: "not_determined" }))).toMatch(/^Not yet allowed/);
    expect(describeSystem(known({ alert_style: "none" }))).toMatch(/alert style is None/);
    expect(describeSystem(known({ alerts: false }))).toBe("Allowed, but alerts are off in System Settings");
  });

  test("lists how an allowed notification shows", () => {
    expect(describeSystem(known({}))).toBe("Allowed: Banners, sound on, badge");
    expect(describeSystem(known({ alert_style: "alert", sound: false, badge: false }))).toBe(
      "Allowed: Alerts, sound off in System Settings, no badge",
    );
  });
});
