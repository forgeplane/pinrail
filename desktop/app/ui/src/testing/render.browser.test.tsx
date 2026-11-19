// The test helpers themselves: a component rendered with the app's
// providers reads from the fake core and hears its notices.
import { useEffect, useState } from "react";
import { afterEach, expect, test, vi } from "vitest";
import { page } from "vitest/browser";
import { api } from "../api/client";
import type { Notice } from "../api/types";
import { useConnected, useNotice } from "../state/live";
import { core } from "./core";
import { cleanup, render } from "./render";

vi.mock("../api/client", async (real) => (await import("./core")).fakeClient(await real()));

afterEach(() => {
  cleanup();
  core.reset();
});

function Probe() {
  const connected = useConnected();
  const [count, setCount] = useState<number | null>(null);
  const [heard, setHeard] = useState<string[]>([]);
  useEffect(() => {
    void api.items().then((d) => setCount(d.items.length));
  }, []);
  useNotice((notice) => setHeard((h) => [...h, notice.kind]));
  return (
    <p>
      {connected ? "Connected" : "Connecting"}, {count ?? "loading"} items, heard {heard.join(" ") || "nothing"}
    </p>
  );
}

test("a component reads what the fake core answers and hears the notices it sends", async () => {
  core.answer("items", { items: [{ key: "PIN-1" }, { key: "PIN-2" }] });
  await render(<Probe />);
  await expect.element(page.getByText("Connected, 2 items, heard nothing")).toBeVisible();
  core.notice({ kind: "item_changed" } as Notice);
  await expect.element(page.getByText("Connected, 2 items, heard item_changed")).toBeVisible();
  // the live provider asks for the items too, for its counts
  expect(core.calls("items")).toContainEqual([]);
});

test("a call the test did not answer stays loading", async () => {
  await render(<Probe />);
  await expect.element(page.getByText("Connected, loading items, heard nothing")).toBeVisible();
});
