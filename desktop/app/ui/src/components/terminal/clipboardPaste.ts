// ⌘V in a terminal, as herdr pastes: the paths of files copied in Finder,
// quoted for the shell, as if they were dropped on it; otherwise the
// clipboard's text; and an image alone as Ctrl+V, so that the agent reads
// the image from the clipboard itself, as it does when the person presses
// Ctrl+V. Outside the app, a paste of text alone is left to the renderer.
// Files dropped on a terminal paste their paths the same way.

import { inTauri } from "../../api/client";
import { quotePath } from "../../lib/shell";
import type { TerminalView } from "./view";

/** The paths of the files copied to the clipboard, which only the app can
 *  read: none outside it. */
export async function copiedFiles(): Promise<string[]> {
  if (!inTauri()) return [];
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    return await invoke<string[]>("clipboard_files");
  } catch {
    return [];
  }
}

/** The paths of the files of the last drag, which only the app can read:
 *  none outside it. */
export async function draggedFiles(): Promise<string[]> {
  if (!inTauri()) return [];
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    return await invoke<string[]>("dragged_files");
  } catch {
    return [];
  }
}

/** The paths of a paste's `file:` URLs, as Linux file managers copy files. */
function uriListPaths(data: DataTransfer): string[] {
  return data
    .getData("text/uri-list")
    .split(/\r?\n/)
    .filter((line) => line.startsWith("file://"))
    .map((line) => decodeURIComponent(new URL(line).pathname));
}

/** Takes a paste from the renderer and pastes it into `view`: always in
 *  the app, which can read the files copied to the clipboard, and
 *  elsewhere only when it carries files or an image. */
export function pasteInto(
  view: Pick<TerminalView, "paste" | "pressKey">,
  event: ClipboardEvent,
  { files = copiedFiles, native = inTauri() }: { files?: () => Promise<string[]>; native?: boolean } = {},
) {
  const data = event.clipboardData;
  if (!data) return;
  const image = [...data.items].some((item) => item.kind === "file" && item.type.startsWith("image/"));
  if (!native && !data.types.includes("Files") && !image) return;
  event.preventDefault();
  event.stopImmediatePropagation();
  // what the event carries is read before it ends
  const text = data.getData("text/plain");
  const listed = uriListPaths(data);
  void (native ? files() : Promise.resolve([])).then((paths) => {
    const copied = paths.length ? paths : listed;
    if (copied.length) view.paste(copied.map(quotePath).join(" "));
    else if (text) view.paste(text);
    else if (image) view.pressKey({ key: "v", code: "KeyV", ctrlKey: true });
  });
}

/** Lets files be dropped on the terminal. */
export function allowFileDrop(event: DragEvent) {
  if (event.dataTransfer?.types.includes("Files")) event.preventDefault();
}

/** Pastes the paths of the files dropped on `view`, quoted for the shell,
 *  as one paste, and gives it the focus. A drop of anything but files is
 *  left alone. */
export function dropInto(
  view: Pick<TerminalView, "paste" | "focus">,
  event: DragEvent,
  { files = draggedFiles, native = inTauri() }: { files?: () => Promise<string[]>; native?: boolean } = {},
) {
  const data = event.dataTransfer;
  if (!data?.types.includes("Files")) return;
  event.preventDefault();
  event.stopPropagation();
  // what the event carries is read before it ends
  const listed = uriListPaths(data);
  void (native ? files() : Promise.resolve([])).then((paths) => {
    const dropped = paths.length ? paths : listed;
    if (!dropped.length) return;
    view.paste(dropped.map(quotePath).join(" "));
    view.focus();
  });
}
