import { describe, expect, test } from "vitest";
import type { ServerSettings } from "../api/types";
import { fromServer, mergeSettings, patchBody } from "./settings";

// what an older core answers: no inbox, items, plugins or links sections
const served = fromServer({
  appearance: { theme: "dark", dark_theme: "dark", light_theme: "light", text_size: "default" },
  sidebar: { open: true, sessions: { archived: true } },
  close_window: "quit",
  menu_bar_icon: true,
  notifications: { enabled: true, paused_until: null, sound: true, muted_plugins: [] },
  shortcut: { global: "", global_opens: "oldest" },
  port: 4747,
} as unknown as ServerSettings);

describe("fromServer", () => {
  test("gives a missing section its default", () => {
    expect(served.inbox).toEqual({ open_in: "panel", layout: "repository" });
    expect(served.items).toEqual({ layout: "project", nesting: "nested", ordering: "oldest", close_parents: true });
    expect(served.plugins).toEqual({});
    expect(served.links).toEqual({});
  });
  test("merges the sidebar's sessions with their defaults", () => {
    expect(served.sidebar.sessions.archived).toBe(true);
    expect(served.sidebar.sessions.collapsed).toEqual([]);
  });
});

describe("mergeSettings", () => {
  test("merges a section key by key, and the sidebar's sessions within it", () => {
    const next = mergeSettings(served, {
      appearance: { text_size: "large" },
      sidebar: { sessions: { collapsed: ["acme/api"] } },
    });
    expect(next.appearance).toEqual({ ...served.appearance, text_size: "large" });
    expect(next.sidebar.open).toBe(true);
    expect(next.sidebar.sessions).toEqual({ ...served.sidebar.sessions, collapsed: ["acme/api"] });
  });
  test("keeps a value patched to false, and the port", () => {
    const next = mergeSettings(served, { menu_bar_icon: false });
    expect(next.menu_bar_icon).toBe(false);
    expect(next.port).toBe(4747);
  });
  test("merges a plugin's settings per key, and removes a link set to null", () => {
    const base = mergeSettings(served, {
      plugins: { list: { mode: "a", size: 2 } },
      links: { review: { path: "/a" }, list: { path: "/b" } },
    } as never);
    const next = mergeSettings(base, { plugins: { list: { mode: "b" } }, links: { review: null } } as never);
    expect(next.plugins.list).toEqual({ mode: "b", size: 2 });
    expect(Object.keys(next.links)).toEqual(["list"]);
  });
});

test("patchBody sends only the sections a patch changes", () => {
  expect(patchBody({ inbox: { layout: "list" } })).toEqual({ inbox: { layout: "list" } });
  expect(patchBody({ menu_bar_icon: false, port: 0 } as never)).toEqual({ menu_bar_icon: false, port: 0 });
  expect(patchBody({})).toEqual({});
});
