// The renderers the window can use, each an adapter in a folder of its
// own, registered here by its id. An adapter's module is imported only
// when the factory tries it.

import type { Registry } from "../view";

export const RENDERERS: Registry = {
  ghostty: () => import("./ghostty/view").then((m) => m.ghosttyRenderer),
  xterm: () => import("./xterm").then((m) => m.xtermRenderer),
};
