// Copying text: through Tauri's clipboard inside the app, where the web
// clipboard API is not reliably granted, and through the browser's
// otherwise. Throws when neither works, so a button can say so.

import { inTauri } from "../api/client";

export async function copyText(text: string): Promise<void> {
  if (inTauri()) {
    const { writeText } = await import("@tauri-apps/plugin-clipboard-manager");
    await writeText(text);
    return;
  }
  await navigator.clipboard.writeText(text);
}
