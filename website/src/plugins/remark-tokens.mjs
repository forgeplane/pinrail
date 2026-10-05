// The SDK's colour tokens, as swatches in both themes, read from
// pinrail-plugin/src/tokens.css when the site builds so the page
// cannot drift from it. Written as `![alt](tokens:)` in a docs page.
import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { visit } from "unist-util-visit";

const stylesheet = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../../../pinrail-plugin/src/tokens.css");

/** The stylesheet's hash, given to the plugin as an option. Astro keeps a
 *  page's rendered HTML until the page or the site's config changes, and the
 *  options are part of the config, so a change to the tokens renders the
 *  page again. */
export const stylesheetDigest = crypto.createHash("sha256").update(fs.readFileSync(stylesheet)).digest("hex");

const escape = (s) =>
  String(s).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");

/** What each token is for, in the groups a view reaches for them. */
const GROUPS = [
  [
    "Surfaces",
    [
      ["--pinrail-bg", "The page"],
      ["--pinrail-bg-panel", "Headers, items"],
      ["--pinrail-bg-raised", "Something above the page: a card, a popover"],
      ["--pinrail-bg-hover", "Under the pointer"],
    ],
  ],
  [
    "Lines",
    [
      ["--pinrail-border", "Dividers and quiet outlines"],
      ["--pinrail-border-strong", "Controls and fields"],
    ],
  ],
  [
    "Text",
    [
      ["--pinrail-text", "What is read"],
      ["--pinrail-dim", "Secondary text"],
      ["--pinrail-faint", "Labels, hints, ids"],
    ],
  ],
  [
    "Accent",
    [
      ["--pinrail-accent", "Links, focus, what is selected"],
      ["--pinrail-accent-bg", "Behind what is selected"],
      ["--pinrail-button-bg", "The one primary button"],
    ],
  ],
  [
    "Tones",
    [
      ["--pinrail-danger", "Refused, destructive, a blocker"],
      ["--pinrail-warning", "Needs attention"],
      ["--pinrail-info", "Worth knowing"],
      ["--pinrail-success", "Done, accepted"],
      ["--pinrail-neutral", "Neither"],
    ],
  ],
  [
    "Diffs",
    [
      ["--pinrail-add-bg", "An added line"],
      ["--pinrail-add-gut", "Its gutter"],
      ["--pinrail-del-bg", "A removed line"],
      ["--pinrail-del-gut", "Its gutter"],
    ],
  ],
];

/** The tokens a block of the stylesheet sets, `--name: value;` each. A
 *  token block holds no braces of its own, so it ends at the first `}`. */
function tokensIn(css, selector) {
  const at = css.indexOf(`${selector} {`);
  const block = css.slice(at, css.indexOf("}", at));
  return Object.fromEntries([...block.matchAll(/(--[a-z0-9-]+):\s*([^;]+);/g)].map((m) => [m[1], m[2].trim()]));
}

function swatches() {
  const css = fs.readFileSync(stylesheet, "utf8");
  const dark = tokensIn(css, ":root");
  const light = { ...dark, ...tokensIn(css, '[data-theme="light"]') };
  const swatch = (value) =>
    `<span class="pr-swatch"><span class="pr-swatch-chip" style="background:${escape(value)}"></span><code>${escape(value)}</code></span>`;
  const rows = GROUPS.map(([group, tokens]) => {
    const found = tokens.filter(([name]) => dark[name]);
    return `<tr class="pr-tokens-group"><th colspan="4">${escape(group)}</th></tr>${found
      .map(
        ([name, role]) =>
          `<tr><td><code>${escape(name)}</code></td><td>${escape(role)}</td><td class="pr-tokens-light">${swatch(light[name])}</td><td class="pr-tokens-dark">${swatch(dark[name])}</td></tr>`,
      )
      .join("")}`;
  }).join("");
  return `<div class="pr-tokens not-content"><table><colgroup><col class="pr-tokens-name"><col><col class="pr-tokens-swatch"><col class="pr-tokens-swatch"></colgroup><thead><tr><th>Token</th><th>For</th><th>Light</th><th>Dark</th></tr></thead><tbody>${rows}</tbody></table></div>`;
}

const isTokens = (child) => child.type === "image" && child.url === "tokens:";

export default function remarkTokens() {
  return (tree) => {
    visit(tree, "paragraph", (node, index, parent) => {
      if (!parent) return;
      const found = node.children.filter(isTokens);
      const rest = node.children.filter((c) => !isTokens(c) && !(c.type === "text" && !c.value.trim()));
      if (found.length !== 1 || rest.length) return;
      parent.children[index] = { type: "html", value: swatches() };
    });
  };
}
