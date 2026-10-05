// The site: the landing page at /, the docs under /docs from the markdown
// in ../docs. Starlight renders the docs; the landing page is its own.
import { defineConfig } from "astro/config";
import starlight from "@astrojs/starlight";
import starlightLinksValidator from "starlight-links-validator";
import tailwindcss from "@tailwindcss/vite";
import { unified } from "@astrojs/markdown-remark";
import remarkMermaid from "./src/plugins/remark-mermaid.mjs";
import remarkScreenshots from "./src/plugins/remark-screenshots.mjs";
import remarkVideo from "./src/plugins/remark-video.mjs";
import remarkKbd from "./src/plugins/remark-kbd.mjs";
import remarkPageSlug from "./src/plugins/remark-page-slug.mjs";
import remarkContract from "./src/plugins/remark-contract.mjs";
import remarkTokens, { stylesheetDigest } from "./src/plugins/remark-tokens.mjs";
import remarkExamples from "./src/plugins/remark-examples.mjs";

export default defineConfig({
  site: "https://pinrail.dev",
  vite: { plugins: [tailwindcss()] },
  // unified, so the docs' ```mermaid blocks become diagrams, screenshot:
  // images the app in the reader's theme, and contract: images a plugin's
  // manifest and schemas; Starlight adds its
  // own plugins (asides, heading links) to the same processor
  markdown: {
    processor: unified({
      remarkPlugins: [
        remarkPageSlug,
        remarkMermaid,
        remarkScreenshots,
        remarkVideo,
        remarkKbd,
        remarkContract,
        [remarkTokens, { stylesheet: stylesheetDigest }],
        remarkExamples,
      ],
    }),
  },
  integrations: [
    starlight({
      title: "Pinrail",
      description: "The inbox where your agents ask before they act.",
      // a link to a page or heading that does not exist fails the build;
      // the download page is the site's own, and the docs name the local
      // server's address on purpose
      plugins: [starlightLinksValidator({ exclude: ["/download/"], errorOnLocalLinks: false })],
      customCss: ["./src/styles/docs.css"],
      // the docs live beside the code, so Starlight's asides and heading links must reach them there
      markdown: { processedDirs: ["../docs"] },
      components: {
        Header: "./src/components/docs/Header.astro",
        Sidebar: "./src/components/docs/Sidebar.astro",
        SiteTitle: "./src/components/docs/SiteTitle.astro",
        ThemeSelect: "./src/components/docs/ThemeToggle.astro",
        Pagination: "./src/components/docs/Pagination.astro",
      },
      head: [
        { tag: "script", attrs: { src: "/docs.js", defer: true } },
        {
          tag: "link",
          attrs: {
            rel: "stylesheet",
            href: "https://fonts.googleapis.com/css2?family=Geist:wght@400..800&family=Newsreader:ital,opsz,wght@0,6..72,400..700;1,6..72,400..600&family=JetBrains+Mono:wght@400..600&display=swap",
          },
        },
      ],
      // code in the site's colours: the terminal's warm dark, and the paper
      expressiveCode: {
        themes: ["vitesse-dark", "vitesse-light"],
        // commands and code stay readable on both backgrounds
        minSyntaxHighlightingColorContrast: 7,
        styleOverrides: {
          borderRadius: "10px",
          borderColor: ({ theme }) => (theme.type === "dark" ? "#34302b" : "#ddd4c3"),
          codeBackground: ({ theme }) => (theme.type === "dark" ? "#161412" : "#fffdf8"),
          codeFontFamily: "'JetBrains Mono', ui-monospace, monospace",
          codeFontSize: "0.84rem",
          codeLineHeight: "1.7",
          uiFontFamily: "Geist, system-ui, sans-serif",
          textMarkers: {
            markBackground: ({ theme }) => (theme.type === "dark" ? "#e5694f22" : "#cf3a1f14"),
            markBorderColor: ({ theme }) => (theme.type === "dark" ? "#e5694f" : "#cf3a1f"),
          },
          frames: {
            editorTabBarBackground: ({ theme }) => (theme.type === "dark" ? "#201d1a" : "#f2ece1"),
            editorActiveTabBackground: ({ theme }) => (theme.type === "dark" ? "#161412" : "#fffdf8"),
            editorActiveTabIndicatorTopColor: "#cf3a1f",
            editorActiveTabIndicatorBottomColor: "transparent",
            // a terminal is dark in both themes, as the site draws its terminals
            terminalTitlebarBackground: "#2a2723",
            terminalBackground: "#1c1a17",
            terminalTitlebarDotsForeground: "#4a453e",
            terminalTitlebarForeground: "#928a7c",
            terminalTitlebarBorderBottomColor: "#34302b",
            frameBoxShadowCssValue: "none",
          },
        },
      },
      social: [{ icon: "github", label: "GitHub", href: "https://github.com/forgeplane/pinrail" }],
      sidebar: [
        {
          label: "Getting started",
          items: [
            { label: "Introduction", slug: "docs" },
            "docs/getting-started/install",
            "docs/getting-started/first-review",
          ],
        },
        { label: "Concepts", items: ["docs/concepts/reviews", "docs/concepts/plugins", "docs/concepts/trust"] },
        {
          label: "Using Pinrail",
          items: [
            "docs/using/inbox",
            "docs/using/installing-plugins",
            "docs/using/settings",
            "docs/using/notifications",
          ],
        },
        { label: "For agents", items: ["docs/agents/instructing", "docs/agents/cli", "docs/agents/workflows"] },
        {
          label: "Plugins",
          items: [
            { label: "Overview", slug: "docs/plugins" },
            "docs/plugins/list",
            "docs/plugins/feedback",
            "docs/plugins/code-review",
            "docs/plugins/image",
            "docs/plugins/markdown",
            "docs/plugins/email",
            "docs/plugins/artifact",
            "docs/plugins/calendar",
            "docs/plugins/logo",
            "docs/plugins/model",
          ],
        },
        {
          label: "Building plugins",
          items: [
            "docs/building/writing",
            "docs/building/design",
            "docs/building/settings-and-keys",
            "docs/building/frameworks",
            "docs/building/protocol",
            "docs/building/publishing",
          ],
        },
        { label: "Reference", items: ["docs/reference/cli", "docs/reference/settings", "docs/reference/manifest"] },
      ],
    }),
  ],
});
