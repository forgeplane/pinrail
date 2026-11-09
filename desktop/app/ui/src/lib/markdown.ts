// Markdown an agent wrote, such as a question's context, rendered for the
// app's own window. Raw HTML is shown as text, and only web and mail links
// are kept, so nothing an agent writes runs here.

import MarkdownIt from "markdown-it";
import { useSyncExternalStore } from "react";
import { highlight } from "./highlight";

// a block of code in a language it names is highlighted; markdown-it
// escapes any other
const md = new MarkdownIt({ html: false, linkify: true, highlight: (code, lang) => highlight(code, lang) ?? "" });
const LINK = /^(https?:|mailto:)/i;
md.validateLink = (url) => LINK.test(url.trim());

// A work item's key, such as PIN-42, in plain text becomes a link to the
// item when its prefix is a repository's; in code and in links it stays
// as written.
let prefixes = new Set<string>();
let prefixVersion = 0;
const prefixListeners = new Set<() => void>();
const KEY = /\b([A-Z]{2,5})-([1-9][0-9]{0,8})\b/g;

/** The repositories' key prefixes the app knows, for the links to items. */
export function setItemPrefixes(next: string[]) {
  const set = new Set(next);
  if (set.size === prefixes.size && [...set].every((p) => prefixes.has(p))) return;
  prefixes = set;
  prefixVersion++;
  for (const listen of prefixListeners) listen();
}

/** Calls `listen` when the prefixes change; returns how to stop. */
export function onItemPrefixes(listen: () => void): () => void {
  prefixListeners.add(listen);
  return () => prefixListeners.delete(listen);
}

export const itemPrefixesVersion = () => prefixVersion;

type Rules = (typeof md)["core"]["ruler"];
function linkKeys(rules: Rules) {
  rules.push("item_keys", (state) => {
    if (!prefixes.size) return;
    for (const block of state.tokens) {
      if (block.type !== "inline" || !block.children) continue;
      const out: typeof block.children = [];
      let inLink = 0;
      for (const token of block.children) {
        if (token.type === "link_open") inLink++;
        if (token.type === "link_close") inLink--;
        if (token.type !== "text" || inLink > 0) {
          out.push(token);
          continue;
        }
        let at = 0;
        for (const match of token.content.matchAll(KEY)) {
          if (!prefixes.has(match[1])) continue;
          const start = match.index ?? 0;
          if (start > at) {
            const before = new state.Token("text", "", 0);
            before.content = token.content.slice(at, start);
            out.push(before);
          }
          const open = new state.Token("link_open", "a", 1);
          open.attrSet("href", `#/items?open=${match[0]}`);
          open.attrSet("class", "item-ref");
          open.attrSet("data-item", match[0]);
          const text = new state.Token("text", "", 0);
          text.content = match[0];
          out.push(open, text, new state.Token("link_close", "a", -1));
          at = start + match[0].length;
        }
        if (at === 0) out.push(token);
        else if (at < token.content.length) {
          const rest = new state.Token("text", "", 0);
          rest.content = token.content.slice(at);
          out.push(rest);
        }
      }
      block.children = out;
    }
  });
}
linkKeys(md.core.ruler);

export function renderMarkdown(text: string): string {
  return md.render(text);
}

// a note's Markdown can also name the files it carries, as attachment:NAME
const noteMd = new MarkdownIt({ html: false, linkify: true, highlight: (code, lang) => highlight(code, lang) ?? "" });
const NOTE_LINK = /^(https?:|mailto:|attachment:)/i;
noteMd.validateLink = (url) => NOTE_LINK.test(url.trim());
const ATTACHMENT = "attachment:";
type NoteEnv = { fileUrl: (name: string) => string | null };
const fileName = (url: string) => decodeURIComponent(url.slice(ATTACHMENT.length));

// an image of the note's own shows from its address, and is left out
// without one
noteMd.renderer.rules.image = (tokens, idx, options, env, self) => {
  const token = tokens[idx];
  const src = String(token.attrGet("src") ?? "");
  if (!src.startsWith(ATTACHMENT)) return self.renderToken(tokens, idx, options);
  const url = (env as NoteEnv).fileUrl(fileName(src));
  if (!url) return "";
  token.attrSet("src", url);
  token.attrSet("data-attachment", fileName(src));
  token.attrSet("alt", token.content);
  return self.renderToken(tokens, idx, options);
};
// a link to one of its files saves it, which the thread does on a click
noteMd.renderer.rules.link_open = (tokens, idx, options, _env, self) => {
  const token = tokens[idx];
  const href = String(token.attrGet("href") ?? "");
  if (href.startsWith(ATTACHMENT)) {
    token.attrSet("href", "#");
    token.attrSet("data-attachment", fileName(href));
  }
  return self.renderToken(tokens, idx, options);
};

linkKeys(noteMd.core.ruler);

/** A note's Markdown, with the files it names at the addresses `fileUrl`
 *  gives. */
export function renderNote(text: string, fileUrl: (name: string) => string | null): string {
  return noteMd.render(text, { fileUrl } satisfies NoteEnv);
}

/** One line of Markdown, such as a question: code, emphasis and links, with
 *  no paragraphs, headings or lists. */
export function renderInline(text: string): string {
  return md.renderInline(text);
}

// plain text with its web links, for what an agent reports as text
const textMd = new MarkdownIt("zero", { linkify: true }).enable(["linkify"]);
textMd.validateLink = md.validateLink;

/** Text that is not Markdown, escaped, with its web links as links. */
export function renderLinks(text: string): string {
  return textMd.renderInline(text);
}

/** The line as it reads without its marks, for a label or a tooltip. */
export function plainInline(text: string): string {
  return new DOMParser().parseFromString(md.renderInline(text), "text/html").body.textContent ?? text;
}

/** A number that changes when the known key prefixes do, for a rendering
 *  to depend on. */
export function useItemPrefixes(): number {
  return useSyncExternalStore(onItemPrefixes, itemPrefixesVersion);
}

/** How a text is rendered: Markdown, a note with its files, one line of
 *  Markdown, or plain text with its links. */
export type MarkdownMode = "markdown" | "note" | "inline" | "links";

/** Renders `text` as `mode` says. `prefixes` is the version of the key
 *  prefixes the rendering is for, from useItemPrefixes: a memo of the HTML
 *  depends on it, and renders again when it changes. */
export function renderAs(
  mode: MarkdownMode,
  text: string,
  prefixes: number,
  fileUrl: (name: string) => string | null = () => null,
): string {
  void prefixes;
  switch (mode) {
    case "note":
      return renderNote(text, fileUrl);
    case "inline":
      return renderInline(text);
    case "links":
      return renderLinks(text);
    default:
      return renderMarkdown(text);
  }
}
