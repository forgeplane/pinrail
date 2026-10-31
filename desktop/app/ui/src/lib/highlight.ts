// Code in the Markdown an agent wrote, highlighted in the languages agents
// use most. Only a block that names its language is highlighted: guessing
// it is slow and often wrong.

import hljs from "@highlightjs/cdn-assets/es/core.min.js";
import bash from "@highlightjs/cdn-assets/es/languages/bash.min.js";
import css from "@highlightjs/cdn-assets/es/languages/css.min.js";
import diff from "@highlightjs/cdn-assets/es/languages/diff.min.js";
import elixir from "@highlightjs/cdn-assets/es/languages/elixir.min.js";
import go from "@highlightjs/cdn-assets/es/languages/go.min.js";
import javascript from "@highlightjs/cdn-assets/es/languages/javascript.min.js";
import json from "@highlightjs/cdn-assets/es/languages/json.min.js";
import python from "@highlightjs/cdn-assets/es/languages/python.min.js";
import rust from "@highlightjs/cdn-assets/es/languages/rust.min.js";
import shell from "@highlightjs/cdn-assets/es/languages/shell.min.js";
import sql from "@highlightjs/cdn-assets/es/languages/sql.min.js";
import typescript from "@highlightjs/cdn-assets/es/languages/typescript.min.js";
import xml from "@highlightjs/cdn-assets/es/languages/xml.min.js";
import yaml from "@highlightjs/cdn-assets/es/languages/yaml.min.js";

const LANGUAGES = { bash, css, diff, elixir, go, javascript, json, python, rust, shell, sql, typescript, xml, yaml };
for (const [name, grammar] of Object.entries(LANGUAGES)) hljs.registerLanguage(name, grammar);

/** The code as highlighted HTML, or null when the language is not known. */
export function highlight(code: string, language: string): string | null {
  if (!language || !hljs.getLanguage(language)) return null;
  return hljs.highlight(code, { language, ignoreIllegals: true }).value;
}
