// A hand-drawn diagram in the docs. Written as
// `![alt](diagram:how-pinrail-works "caption")`, alone in its paragraph, it
// becomes a figure with src/diagrams/how-pinrail-works.svg in the page
// itself, so the stylesheet (docs.css) colours it for the reader's theme and
// sets its text in the site's fonts. A drawing for narrow pages, in
// how-pinrail-works-narrow.svg, goes in beside it when there is one, and the
// stylesheet shows the one that fits. The alt text names each drawing, the
// title is the caption, and a click opens the diagram full size, as it does
// a ```mermaid diagram (public/docs.js).
import { existsSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { visit } from "unist-util-visit";

const folder = new URL("../diagrams/", import.meta.url);

const escape = (s) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");

// the drawing as an element of the page: no XML prolog or comments, and the
// class, role and name the page needs on its root
const drawing = (path, layout, alt) =>
  readFileSync(path, "utf8")
    .replace(/<\?xml[^>]*\?>|<!--[\s\S]*?-->/g, "")
    .trim()
    .replace(/^<svg\b/, `<svg class="pr-diagram-svg pr-diagram-${layout}" role="img" aria-label="${alt}"`);

const isDiagram = (child) => child.type === "image" && child.url.startsWith("diagram:");

export default function remarkDiagram() {
  return (tree, file) => {
    visit(tree, "paragraph", (node, index, parent) => {
      if (!parent || node.children.length !== 1 || !isDiagram(node.children[0])) return;
      const [image] = node.children;
      const name = image.url.slice("diagram:".length);
      const wide = fileURLToPath(new URL(`${name}.svg`, folder));
      const narrow = fileURLToPath(new URL(`${name}-narrow.svg`, folder));
      if (!/^[a-z0-9-]+$/.test(name) || !existsSync(wide)) {
        file.message(`there is no diagram named ${name} in src/diagrams`, node);
        return;
      }
      const alt = escape(image.alt ?? "");
      const title = image.title ? escape(image.title) : "";
      const both = existsSync(narrow);
      parent.children[index] = {
        type: "html",
        value:
          `<figure class="pr-mermaid pr-diagram${both ? " pr-diagram-fits" : ""}"${title ? ` aria-label="${title}"` : ""}>` +
          `<div class="pr-mermaid-canvas">` +
          drawing(wide, "wide", alt) +
          (both ? drawing(narrow, "narrow", alt) : "") +
          `<button type="button" class="pr-diagram-open" aria-label="Enlarge the diagram${title ? `: ${title}` : ""}">Click to enlarge</button>` +
          `</div>${title ? `<figcaption>${title}</figcaption>` : ""}</figure>`,
      };
    });
  };
}
