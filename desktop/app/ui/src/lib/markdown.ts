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
