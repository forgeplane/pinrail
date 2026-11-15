// The libghostty view restores the host's binary snapshot exactly.
import { expect, test } from "vitest";
import snapshotUrl from "../../testing/ghostty-snapshot.bin?url";
import host from "../../testing/ghostty-snapshot.json";
import { GhosttyCore } from "./core";
import { GhosttyRuntime } from "./runtime";
import { BINARY_FORMAT } from "./view";

test("a snapshot the host wrote shows the host's screen, history and title", async () => {
  expect(host.format).toBe(BINARY_FORMAT);
  const bytes = new Uint8Array(await (await fetch(snapshotUrl)).arrayBuffer());
  const theme = { foreground: { r: 1, g: 1, b: 1 }, background: { r: 0, g: 0, b: 0 }, cursor: { r: 1, g: 1, b: 1 } };
  const core = GhosttyCore.create(await GhosttyRuntime.instantiate(), 80, 24, 8, 16, theme, () => {});
  core.restoreSnapshot(bytes, 80, 24, 8, 16);
  const { total, length } = core.scrollbar();
  const retained = total - length;
  const lines = core.screenLines();
  expect(retained).toBe(host.retainedLines);
  expect(lines.slice(0, retained).join("\n")).toBe(host.history);
  expect(lines.slice(retained).join("\n").trimEnd()).toBe(host.screen.trimEnd());
  expect(core.title()).toBe(host.title);
  core.dispose();
});
