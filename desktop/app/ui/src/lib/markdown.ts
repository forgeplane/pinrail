// Markdown an agent wrote, such as a question's context, rendered for the
// app's own window. Raw HTML is shown as text, and only web and mail links
// are kept, so nothing an agent writes runs here.

import MarkdownIt from "markdown-it";

const md = new MarkdownIt({ html: false, linkify: true });
const LINK = /^(https?:|mailto:)/i;
md.validateLink = (url) => LINK.test(url.trim());

export function renderMarkdown(text: string): string {
  return md.render(text);
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
