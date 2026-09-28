// Keyboard shortcuts in the docs, one way everywhere. Written as
// `<kbd>⌘⇧M</kbd>`, `<kbd>shift+s</kbd>` or `<kbd>j</kbd>`, a shortcut becomes
// one keycap with the Mac's symbols, its keys joined by +: ⇧ + S, ⌘ + Enter,
// letters in capitals.
import { visit } from "unist-util-visit";

const NAMES = {
  cmd: "⌘",
  command: "⌘",
  meta: "⌘",
  ctrl: "Ctrl",
  control: "Ctrl",
  alt: "⌥",
  option: "⌥",
  opt: "⌥",
  shift: "⇧",
  enter: "Enter",
  return: "Enter",
  esc: "Esc",
  escape: "Esc",
  tab: "Tab",
  space: "Space",
  backspace: "Delete",
  delete: "Delete",
  up: "↑",
  down: "↓",
  left: "←",
  right: "→",
};
const SYMBOLS = new Set(["⌘", "⌥", "⇧", "↵", "↑", "↓", "←", "→", "⌫"]);
// ↵ and ⌫ are thin and sit low in the docs' type; the words read better
const WORDS = { "↵": "Enter", "⌫": "Delete" };
const LABELS = { "⌘": "Command", "⌥": "Option", "⇧": "Shift", "↑": "Up", "↓": "Down", "←": "Left", "→": "Right" };
const escape = (s) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");

/* "shift+s" → ["⇧", "S"]; "⌘⇧M" → ["⌘", "⇧", "M"]; "Esc" → ["Esc"] */
export function keys(text) {
  const out = [];
  for (const part of text.trim().split(/\s*\+\s*(?=.)/)) {
    const named = NAMES[part.toLowerCase()];
    if (named) {
      out.push(named);
      continue;
    }
    // a run of symbols and one key, as ⌘⇧M or ⌥↓
    let rest = part;
    while (rest && SYMBOLS.has(rest[0]) && rest.length > 1) {
      out.push(WORDS[rest[0]] ?? rest[0]);
      rest = rest.slice(1);
    }
    if (rest) out.push(WORDS[rest] ?? (rest.length === 1 ? rest.toUpperCase() : (NAMES[rest.toLowerCase()] ?? rest)));
  }
  return out;
}

export function kbdHtml(text) {
  const caps = keys(text);
  const spoken = caps.map((k) => LABELS[k] ?? k).join(" ");
  return `<kbd class="pr-keys not-content" title="${escape(spoken)}">${caps.map((k) => escape(k)).join('<span class="pr-plus" aria-hidden="true">+</span>')}</kbd>`;
}

export default function remarkKbd() {
  return (tree) => {
    visit(tree, (node) => {
      if (!Array.isArray(node.children)) return;
      const kids = node.children;
      for (let i = 0; i < kids.length; i++) {
        const open = kids[i];
        if (open.type !== "html" || open.value.trim().toLowerCase() !== "<kbd>") continue;
        const close = kids.findIndex((k, j) => j > i && k.type === "html" && k.value.trim().toLowerCase() === "</kbd>");
        if (close < 0) continue;
        const text = kids
          .slice(i + 1, close)
          .map((k) => k.value ?? "")
          .join("");
        kids.splice(i, close - i + 1, { type: "html", value: kbdHtml(text) });
      }
    });
  };
}
