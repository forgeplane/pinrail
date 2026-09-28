// Code from the example plugins under docs/examples, read when the site
// builds so a page shows exactly what the examples' tests run. A paragraph
// of links, each `![label](example:<path>)`:
//
//   ![React](example:ship-it/react/src/main.tsx) ![React](example:ship-it/react/src/App.tsx)
//   ![Vue](example:ship-it/vue/src/App.vue)
//
// becomes one group with a tab per label, each holding its files as code
// blocks titled with their path. Picking a label in one group picks it in
// every group on the page, and is remembered (public/docs.js). A paragraph
// with a single label is just its code blocks.
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { visit } from "unist-util-visit";

const examples = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../../../docs/examples");

const escape = (s) =>
  String(s).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
const LANGS = {
  ts: "ts",
  tsx: "tsx",
  js: "js",
  mjs: "js",
  vue: "vue",
  svelte: "svelte",
  json: "json",
  html: "html",
  css: "css",
  md: "md",
};
const slug = (label) => label.toLowerCase().replace(/[^a-z0-9]+/g, "-");

function code(file) {
  const full = path.join(examples, file);
  if (!full.startsWith(examples + path.sep)) throw new Error(`example:${file} is outside docs/examples`);
  const lang = LANGS[path.extname(file).slice(1)] ?? "text";
  // the title is the path inside the plugin, as the reader would have it
  const title = file.split("/").slice(2).join("/");
  return { type: "code", lang, meta: `title="${title}"`, value: fs.readFileSync(full, "utf8").replace(/\n$/, "") };
}

const isExample = (child) => child.type === "image" && child.url.startsWith("example:");

export default function remarkExamples() {
  return (tree) => {
    visit(tree, "paragraph", (node, index, parent) => {
      if (!parent) return;
      const found = node.children.filter(isExample);
      const rest = node.children.filter((c) => !isExample(c) && !(c.type === "text" && !c.value.trim()));
      if (!found.length || rest.length) return;

      // the files under each label, labels in the order they first appear
      const groups = new Map();
      for (const image of found) {
        const label = image.alt || "Example";
        if (!groups.has(label)) groups.set(label, []);
        groups.get(label).push(image.url.slice("example:".length));
      }
      const html = (value) => ({ type: "html", value });
      const nodes = [];
      if (groups.size === 1) {
        for (const file of [...groups.values()][0]) nodes.push(code(file));
      } else {
        const labels = [...groups.keys()];
        const tabs = labels
          .map(
            (label, i) =>
              `<button type="button" role="tab" aria-selected="${i === 0}" tabindex="${i === 0 ? 0 : -1}" data-framework-tab="${slug(label)}">${escape(label)}</button>`,
          )
          .join("");
        nodes.push(
          html(
            `<div class="pr-frameworks" data-frameworks><div class="pr-frameworks-tabs not-content" role="tablist" aria-label="Framework">${tabs}</div>`,
          ),
        );
        labels.forEach((label, i) => {
          nodes.push(
            html(
              `<div class="pr-frameworks-panel" role="tabpanel" data-framework-panel="${slug(label)}"${i === 0 ? "" : " hidden"}>`,
            ),
          );
          for (const file of groups.get(label)) nodes.push(code(file));
          nodes.push(html(`</div>`));
        });
        nodes.push(html(`</div>`));
      }
      parent.children.splice(index, 1, ...nodes);
      return index + nodes.length;
    });
  };
}
