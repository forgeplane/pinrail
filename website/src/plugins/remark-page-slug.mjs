// Tells the links validator where a page lives. The docs are read from
// ../docs, beside the code, and each page's id is docs/<its path> (see
// src/content.config.ts). The validator names a page by its path under
// src/content/docs unless the page has a slug, so this gives every page
// the slug it is served at. Starlight routes by the collection's ids, so
// the slug changes nothing else.
import path from "node:path";
import { fileURLToPath } from "node:url";

const docs = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "..", "..", "docs");

export default function remarkPageSlug() {
  return (_tree, file) => {
    const where = file.history[0];
    const frontmatter = file.data.astro?.frontmatter;
    if (!where || !frontmatter || typeof frontmatter.slug === "string") return;
    const relative = path.relative(docs, where);
    if (relative.startsWith("..")) return;
    const page = relative.replace(/\.mdx?$/, "").replace(/(^|\/)index$/, "");
    frontmatter.slug = ["docs", page].filter(Boolean).join("/");
  };
}
