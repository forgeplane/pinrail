// A ```mermaid block in the docs becomes a diagram drawn at build time: an
// SVG in the page, so it shows with the page and needs no script. Each diagram
// is drawn twice, in the docs' light and dark colours, and the stylesheet shows
// the one for the current theme. `title="…"` after the language names the
// figure and shows as its caption. A diagram that fails to draw stays a code
// block, and the build says why.
import { visit } from "unist-util-visit";
import { createMermaidRenderer } from "mermaid-isomorphic";

const escape = (s) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");

// The docs theme's colours (src/styles/docs.css), per theme
const palettes = {
  light: { ink: "#1e1b17", paper: "#fffdf8", line: "#8b8274", rule: "#c9bea9", pen: "#cf3a1f", wash: "#f7dcd3", soft: "#f2ece1", page: "#f8f4ec", dim: "#6f685c" },
  dark: { ink: "#fbf8f2", paper: "#23201c", line: "#6f685d", rule: "#3b3731", pen: "#e5694f", wash: "#43211a", soft: "#2a2723", page: "#1c1a17", dim: "#a1998b" },
};

const config = (theme) => {
  const c = palettes[theme];
  return {
    theme: "base",
    darkMode: theme === "dark",
    // JetBrains Mono has the same advance as the system monospace it falls back
    // to, so labels measured at build time fit when the page draws them
    fontFamily: '"JetBrains Mono", Menlo, Consolas, monospace',
    themeVariables: {
      darkMode: theme === "dark",
      background: "transparent",
      fontFamily: '"JetBrains Mono", Menlo, Consolas, monospace',
      fontSize: "13px",
      primaryColor: c.paper, primaryTextColor: c.ink, primaryBorderColor: c.rule,
      secondaryColor: c.wash, secondaryTextColor: c.ink, secondaryBorderColor: c.pen,
      tertiaryColor: c.soft, tertiaryTextColor: c.ink, tertiaryBorderColor: c.rule,
      lineColor: c.line, textColor: c.ink, mainBkg: c.paper, nodeBorder: c.rule, clusterBkg: "transparent", clusterBorder: c.rule,
      edgeLabelBackground: c.page, titleColor: c.ink,
      actorBkg: c.paper, actorBorder: c.rule, actorTextColor: c.ink, actorLineColor: c.rule,
      signalColor: c.line, signalTextColor: c.ink, labelBoxBkgColor: c.paper, labelBoxBorderColor: c.rule, labelTextColor: c.ink,
      loopTextColor: c.dim, noteBkgColor: c.wash, noteBorderColor: c.pen, noteTextColor: c.ink,
      activationBkgColor: c.wash, activationBorderColor: c.pen, sequenceNumberColor: c.paper,
    },
    flowchart: { curve: "basis", padding: 14 },
    sequence: { mirrorActors: false, messageAlign: "center", actorMargin: 70 },
  };
};

// one headless browser for the whole build, started on the first diagram
let renderer;
const render = (sources, theme, prefix) => (renderer ??= createMermaidRenderer())(sources, { mermaidConfig: config(theme), prefix });

export default function remarkMermaid() {
  return async (tree, file) => {
    const blocks = [];
    visit(tree, "code", (node, index, parent) => {
      if (node.lang === "mermaid" && parent) blocks.push({ node, index, parent });
    });
    if (!blocks.length) return;

    const sources = blocks.map((b) => b.node.value);
    // ids stay unique on the page: one prefix per theme
    const [light, dark] = await Promise.all([render(sources, "light", "pr-mermaid-l"), render(sources, "dark", "pr-mermaid-d")]);

    blocks.forEach(({ node, index, parent }, i) => {
      if (light[i].status !== "fulfilled" || dark[i].status !== "fulfilled") {
        const reason = (light[i].reason ?? dark[i].reason)?.message ?? "unknown error";
        file.message(`mermaid diagram ${i + 1} did not draw: ${reason}`, node);
        return;
      }
      const title = /title="([^"]*)"/.exec(node.meta || "")?.[1];
      parent.children[index] = {
        type: "html",
        value:
          `<figure class="pr-mermaid"${title ? ` aria-label="${escape(title)}"` : ""}>` +
          `<div class="pr-mermaid-canvas" role="button" tabindex="0" aria-label="Enlarge the diagram${title ? `: ${escape(title)}` : ""}">` +
          `<div class="pr-mermaid-light">${light[i].value.svg}</div>` +
          `<div class="pr-mermaid-dark">${dark[i].value.svg}</div>` +
          `<span class="pr-shot-hint" aria-hidden="true">Click to enlarge</span></div>${title ? `<figcaption>${escape(title)}</figcaption>` : ""}</figure>`,
      };
    });
  };
}
