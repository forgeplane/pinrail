// A screenshot of the app in the docs, in the reader's theme. Written as
// `![alt](screenshot:inbox "caption")`, it becomes a figure with both
// /screenshots/inbox-light.png and -dark.png, which `mise run screenshots`
// makes, and the stylesheet shows the one for the current theme. A click opens
// it at full size (public/docs.js). Screenshots written one after another in
// one paragraph become one figure that shows one at a time, each with its own
// caption, with a dot for each; zoomed, arrows move between them.
import { visit } from "unist-util-visit";

const escape = (s) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");

const shotImages = (name, alt) =>
  ["light", "dark"].map((theme) =>
    `<img class="wk-shot-${theme}" src="/screenshots/${name}-${theme}.png" alt="${theme === "light" ? alt : ""}" width="1440" height="900" loading="lazy" decoding="async" />`,
  ).join("");

// the picture is a button: the app is shown whole, so it opens full size to be read
const shotButton = (name, alt) =>
  `<button type="button" class="wk-shot-open" aria-label="Enlarge: ${alt}">${shotImages(name, alt)}<span class="wk-shot-hint" aria-hidden="true">Click to enlarge</span></button>`;

const isShot = (child) => child.type === "image" && child.url.startsWith("screenshot:");

export default function remarkScreenshots() {
  return (tree) => {
    visit(tree, "paragraph", (node, index, parent) => {
      if (!parent) return;
      const shots = node.children.filter(isShot);
      const rest = node.children.filter((c) => !isShot(c) && !(c.type === "text" && !c.value.trim()));
      if (!shots.length || rest.length) return;
      if (shots.length === 1) {
        const [image] = shots;
        const alt = escape(image.alt ?? "");
        parent.children[index] = {
          type: "html",
          value: `<figure class="wk-shot">${shotButton(image.url.slice("screenshot:".length), alt)}${image.title ? `<figcaption>${escape(image.title)}</figcaption>` : ""}</figure>`,
        };
        return;
      }
      // several in one paragraph: one figure showing one at a time, each with
      // its own caption, and a dot for each to move between them
      const slides = shots.map((image, i) => `<div class="wk-slide" data-slide="${i}" ${i === 0 ? "" : "hidden"}>${shotButton(image.url.slice("screenshot:".length), escape(image.alt ?? ""))}${image.title ? `<figcaption>${escape(image.title)}</figcaption>` : ""}</div>`).join("");
      const dots = shots.map((image, i) => `<button type="button" class="wk-dot" data-dot="${i}" aria-label="${escape(`Picture ${i + 1} of ${shots.length}${image.title ? `: ${image.title}` : ""}`)}" aria-current="${i === 0}"></button>`).join("");
      parent.children[index] = {
        type: "html",
        value: `<figure class="wk-shot wk-shots">${slides}<div class="wk-dots not-content" role="group" aria-label="Pictures">${dots}</div></figure>`,
      };
    });
  };
}
