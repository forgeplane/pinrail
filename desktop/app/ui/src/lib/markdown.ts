// Markdown an agent wrote, such as a question's context, rendered for the
// app's own window. Raw HTML is shown as text, and only web and mail links
// are kept, so nothing an agent writes runs here.

import MarkdownIt from "markdown-it";
import { highlight } from "./highlight";

// a block of code in a language it names is highlighted; markdown-it
// escapes any other
const md = new MarkdownIt({ html: false, linkify: true, highlight: (code, lang) => highlight(code, lang) ?? "" });
const LINK = /^(https?:|mailto:)/i;
md.validateLink = (url) => LINK.test(url.trim());

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

/** The line as it reads without its marks, for a label or a tooltip. */
export function plainInline(text: string): string {
  return new DOMParser().parseFromString(md.renderInline(text), "text/html").body.textContent ?? text;
}
