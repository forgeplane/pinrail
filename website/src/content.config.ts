// The docs collection reads the markdown under ../docs, where the writing
// lives beside the code; the site only renders it.
import { defineCollection } from "astro:content";
import { glob } from "astro/loaders";
import { docsSchema } from "@astrojs/starlight/schema";

export const collections = {
  docs: defineCollection({
    // every page lives under /docs; the landing page keeps the root
    loader: glob({
      // the example plugins beside the pages are projects, not pages
      pattern: ["**/[^_]*.{md,mdx}", "!examples/**"],
      base: "../docs",
      // docs/index.md is the /docs page itself, so its id has no trailing slash
      generateId: ({ entry }) =>
        ["docs", entry.replace(/\.mdx?$/, "").replace(/(^|\/)index$/, "")].filter(Boolean).join("/"),
    }),
    schema: docsSchema(),
  }),
};
