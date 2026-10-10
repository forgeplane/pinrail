// The docs collection reads the markdown under ../docs, where the writing
// lives beside the code; the site only renders it.
import { defineCollection } from "astro:content";
import { glob } from "astro/loaders";
import { z } from "astro/zod";
import { i18nLoader } from "@astrojs/starlight/loaders";
import { docsSchema, i18nSchema } from "@astrojs/starlight/schema";

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
  // The blog: one folder per post under src/content/blog, holding the
  // post as index.mdx beside its diagrams. The folder's name is the post's
  // address, /blog/<folder>/.
  blog: defineCollection({
    loader: glob({
      pattern: "*/index.mdx",
      base: "./src/content/blog",
      generateId: ({ entry }) => entry.split("/")[0],
    }),
    schema: z.object({
      title: z.string(),
      /** the line under the title, in italics */
      dek: z.string(),
      /** for search results and link previews */
      description: z.string(),
      /** the day it was published, as YYYY-MM-DD */
      date: z.string().regex(/^\d{4}-\d{2}-\d{2}$/),
      kind: z.string(),
      minutes: z.number().int().positive(),
    }),
  }),
  // Starlight's interface text where the site words it differently, in
  // src/content/i18n/en.json
  i18n: defineCollection({ loader: i18nLoader(), schema: i18nSchema() }),
};
