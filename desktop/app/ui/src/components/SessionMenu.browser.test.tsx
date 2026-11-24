// A session's menu at component level, against the fake core: Copy session
// id copies the agent's own id, and Delete asks first, and Keep it changes
// nothing.
import { useState } from "react";
import { useLocation } from "react-router";
import { afterEach, describe, expect, test, vi } from "vitest";
import { page, userEvent } from "vitest/browser";
import type { Run } from "../api/runtime";
import type { ServerSettings, Session } from "../api/types";
import { core } from "../testing/core";
import { session } from "../testing/items";
import { useToasts } from "../state/toasts";
import { cleanup, render } from "../testing/render";
import { SessionMenu } from "./SessionMenu";

vi.mock("../api/client", async (real) => (await import("../testing/core")).fakeClient(await real()));
const clipboard = vi.hoisted(() => ({ copied: [] as string[] }));
vi.mock("../lib/clipboard", () => ({
  copyText: async (text: string) => void clipboard.copied.push(text),
}));

const app = vi.hoisted(() => ({ runs: [] as Run[] }));
vi.mock("../api/runtime", async (real) => {
  const actual = await real<typeof import("../api/runtime")>();
  return { ...actual, runtime: { ...actual.runtime, runs: async () => ({ runs: app.runs }) } };
});

afterEach(() => {
  app.runs = [];
  cleanup();
  core.reset();
  clipboard.copied = [];
});

function Where() {
  return <output data-testid="where">{useLocation().pathname}</output>;
}

/** The toasts, which the app's layout draws. */
function Toasts() {
  return (
    <output data-testid="toasts">
      {useToasts()
        .toasts.map((t) => t.text)
        .join(" | ")}
    </output>
  );
}

/** The menu as the sidebar holds it: gone once it closes. */
function Menu({
  anchor,
  onClose,
  fields = {},
}: {
  anchor: HTMLElement;
  onClose: () => void;
  fields?: Partial<Session>;
}) {
  const [open, setOpen] = useState(true);
  return open ? (
    <SessionMenu
      session={session({ id: "s-1", title: "Fix the login", ...fields })}
      anchor={anchor}
      onClose={() => {
        setOpen(false);
        onClose();
      }}
    />
  ) : null;
}

/** The menu of session `s-1`, with `fields` in its place. */
async function menu(route = "/", fields: Partial<Session> = {}) {
  core.answer("deleteSession", undefined);
  const anchor = document.createElement("button");
  document.body.append(anchor);
  const onClose = vi.fn();
  await render(
    <>
      <Menu anchor={anchor} onClose={onClose} fields={fields} />
      <Where />
      <Toasts />
    </>,
    { route },
  );
  return { onClose };
}

async function open(route = "/") {
  const { onClose } = await menu(route);
  await userEvent.click(page.getByRole("menuitem", { name: /^Delete/ }));
  return { onClose, dialog: page.getByRole("dialog", { name: "Delete Fix the login?" }) };
}

describe("deleting a session", () => {
  test("asks first, with Keep it focused, and Keep it deletes nothing", async () => {
    const { dialog, onClose } = await open();
    await expect.element(dialog).toBeVisible();
    await expect.element(dialog).toMatchTextContent("Its decided reviews stay in History");
    await expect.element(dialog.getByRole("button", { name: "Keep it" })).toHaveFocus();
    await userEvent.click(dialog.getByRole("button", { name: "Keep it" }));
    expect(core.calls("deleteSession")).toEqual([]);
    expect(onClose).toHaveBeenCalled();
  });

  test("Delete deletes it and says so", async () => {
    const { dialog } = await open();
    await userEvent.click(dialog.getByRole("button", { name: "Delete", exact: true }));
    await expect.poll(() => core.calls("deleteSession")).toEqual([["claude-code", "s-1"]]);
    await expect.element(page.getByTestId("toasts")).toHaveTextContent("Deleted Fix the login");
  });

  test("from the session's own page, leaves it for the Sessions page", async () => {
    const { dialog } = await open("/sessions/claude-code/s-1");
    await userEvent.click(dialog.getByRole("button", { name: "Delete", exact: true }));
    await expect.element(page.getByTestId("where")).toHaveTextContent("/sessions");
  });
});

describe("copying the session id", () => {
  test("copies the agent's own id for the session", async () => {
    await menu("/", { id: "ses_01ABC", agent_session: "3f2a9c1e-5b7d" });
    await userEvent.click(page.getByRole("menuitem", { name: /^Copy session id/ }));
    await expect.poll(() => clipboard.copied).toEqual(["3f2a9c1e-5b7d"]);
    await expect.element(page.getByTestId("toasts")).toHaveTextContent("Copied the session id");
  });

  test("says the id is not known yet while the agent has not reported it", async () => {
    await menu("/", { id: "ses_01ABC", agent_session: null });
    const copy = page.getByRole("menuitem", { name: /^Copy session id/ });
    await expect.element(copy).toBeDisabled();
    await expect.element(copy).toMatchTextContent("Not known yet");
    expect(clipboard.copied).toEqual([]);
  });
});

describe("with the agent runtime on", () => {
  /** A run of Claude Code, live, in session `id`. */
  const live = (id: string): Run => ({
    id: "run_1",
    host: "local",
    agent: "claude-code",
    state: "running",
    terminal: "local:t1",
    folder: "/work/app",
    started_at: "2026-11-19T09:00:00Z",
    ended_at: null,
    end_reason: null,
    exit_code: null,
    exit_signal: null,
    session: { agent: "claude-code", id, agent_session: null },
  });

  test("a run that went on in another session, as after /clear, is not this session's to stop", async () => {
    core.answer("settings", {
      appearance: { theme: "system", dark_theme: "dark", light_theme: "light", text_size: "default" },
      sidebar: { open: true },
      notifications: { enabled: true, paused_until: null, sound: true, muted_plugins: [] },
      shortcut: { global: "", global_opens: "oldest" },
      port: 4747,
      labs: { runtime: true },
    } as unknown as ServerSettings);
    app.runs = [live("s-2")];
    await menu("/", {
      resume: { resume: true, while_running: false },
      presence: { state: "stopped" },
    } as Partial<Session>);
    await expect.element(page.getByRole("menuitem", { name: "Resume Claude Code" })).toBeVisible();
    await expect.element(page.getByRole("menuitem", { name: "Stop Claude Code" })).not.toBeInTheDocument();
  });
});
