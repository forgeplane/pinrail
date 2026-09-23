// A plugin's contract in its docs page, read from the plugin's own files so
// the page cannot drift from them. Written as `![alt](contract:model)`, it
// becomes a figure with tabs for the manifest, the payload schema (what the
// agent sends), the decision schema (what comes back) and, for a plugin that
// declares them, its settings, each drawn as
// fields and one click away from the JSON itself, in the site's code viewer
// (public/docs.js switches between them).
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { visit } from "unist-util-visit";
import { kbdHtml } from "./remark-kbd.mjs";

const plugins = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../../../plugins");

const escape = (s) => String(s).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
/** `code` in a description, as the docs write it elsewhere */
const prose = (s) => escape(s).replace(/`([^`]+)`/g, "<code>$1</code>");
const size = (bytes) => (bytes >= 1024 * 1024 ? `${Math.round(bytes / (1024 * 1024))} MB` : bytes >= 1024 ? `${Math.round(bytes / 1024)} KB` : `${bytes} bytes`);

function read(dir, file) {
  return JSON.parse(fs.readFileSync(path.join(dir, file), "utf8"));
}

/** The schema a manifest key names: inline, or the file its `$ref` points at. */
function document(dir, value) {
  return value && typeof value === "object" && typeof value.$ref === "string" && Object.keys(value).length === 1 ? read(dir, value.$ref) : value ?? {};
}

/** A local `#/$defs/x` reference resolved against the document; anything else as it is. */
function resolve(root, schema) {
  let s = schema;
  for (let hops = 0; s && typeof s.$ref === "string" && s.$ref.startsWith("#/") && hops < 8; hops++) {
    const target = s.$ref.slice(2).split("/").reduce((node, key) => node?.[key], root);
    const { $ref, ...rest } = s;
    s = { ...target, ...rest };
  }
  return s ?? {};
}

const isFile = (s) => s?.type === "object" && s.properties && "$artifact" in s.properties;

/** The name of a local definition a schema refers to: `condition` for `#/$defs/condition`. */
const defName = (raw) => (typeof raw?.$ref === "string" && raw.$ref.startsWith("#/") ? raw.$ref.split("/").pop() : null);
const SCALARS = ["string", "number", "integer", "boolean", "null"];

/** A field's type as a reader would say it. A definition that is an object
 *  or a choice of shapes is called by its name; scalars by their type. */
function typeOf(root, s, raw) {
  if (isFile(s)) return "file";
  if (s.enum) return "enum";
  if (s.const !== undefined) return typeof s.const;
  if (Array.isArray(s.type)) return s.type.join(" | ");
  if (s.type === "array") {
    const items = resolve(root, s.items ?? {});
    if (isFile(items)) return "file[]";
    if (SCALARS.includes(items.type)) return `${items.type}[]`;
    return `${defName(s.items) ?? items.type ?? "any"}[]`;
  }
  const alts = s.oneOf ?? s.anyOf;
  if (alts?.every((alt) => alt?.const !== undefined)) return typeof alts[0].const;
  // a choice of plain types reads as the types themselves: string | boolean
  if (alts?.every((alt) => resolve(root, alt).type !== "object" && !resolve(root, alt).oneOf)) {
    return alts.map((alt) => typeOf(root, resolve(root, alt), alt)).join(" | ");
  }
  const name = SCALARS.includes(s.type) ? null : defName(raw);
  if (alts) return `${name ?? (alts.every((alt) => resolve(root, alt).type === "object") ? "object" : "")}${name || alts.every((alt) => resolve(root, alt).type === "object") ? ", " : ""}one of ${alts.length}`;
  if (name) return name;
  if (s.type) return s.type;
  return "any";
}

/** The rules a field is held to, short: values, bounds, patterns. */
function constraints(s) {
  const out = [];
  if (s.enum) out.push(s.enum.map((v) => `<code>${escape(JSON.stringify(v))}</code>`).join(" "));
  // choices written as oneOf consts, each with the label the app shows
  const consts = (s.oneOf ?? s.anyOf)?.filter((alt) => alt && alt.const !== undefined);
  if (consts?.length && consts.length === (s.oneOf ?? s.anyOf).length) {
    out.push(consts.map((alt) => `<code>${escape(JSON.stringify(alt.const))}</code>${alt.title ? ` ${escape(alt.title)}` : ""}`).join(" · "));
  }
  if (s.const !== undefined) out.push(`<code>${escape(JSON.stringify(s.const))}</code>`);
  if (s.minLength !== undefined || s.maxLength !== undefined) out.push(range("chars", s.minLength, s.maxLength));
  if (s.minimum !== undefined || s.maximum !== undefined) out.push(range("", s.minimum, s.maximum));
  if (s.minItems !== undefined || s.maxItems !== undefined) out.push(range("items", s.minItems, s.maxItems));
  if (s.pattern) out.push(`matches <code>${escape(s.pattern)}</code>`);
  if (s.format) out.push(escape(s.format));
  if (s.contentEncoding) out.push(escape(s.contentEncoding));
  if (s.default !== undefined) out.push(`default <code>${escape(JSON.stringify(s.default))}</code>`);
  return out.filter(Boolean);
}

function range(unit, min, max) {
  const u = unit ? ` ${unit}` : "";
  if (min !== undefined && max !== undefined) return min === max ? `exactly ${min}${u}` : `${min}–${max}${u}`;
  if (min !== undefined) return `at least ${min}${u}`;
  return `at most ${max}${u}`;
}

/** A definition the branch is already inside is not drawn again: the
 *  schema refers to itself (a condition made of conditions). */
const asAbove = (name, each) => `<ul class="pr-fields"><li class="pr-field-note">${each ? "Each one" : "One"} a <code>${escape(name)}</code>, as above.</li></ul>`;

/** The fields an object holds, or an array's items hold, one row each,
 *  nested ones inside. `trail` holds the definitions the branch is inside. */
function children(root, s, depth, trail) {
  let obj = s;
  if (s.type === "array") {
    const ref = s.items?.$ref;
    if (ref && trail.includes(ref)) return asAbove(defName(s.items), true);
    if (ref) trail = [...trail, ref];
    obj = resolve(root, s.items ?? {});
  }
  if (isFile(obj) || depth > 8) return "";
  // an object that is one of several shapes, with no fields of its own: each shape in turn
  const rawAlts = obj.oneOf ?? obj.anyOf;
  const alts = rawAlts?.map((alt) => resolve(root, alt));
  if (!obj.properties && alts?.some((alt) => alt.properties)) {
    const shapes = alts.map((alt, i) => {
      const ref = rawAlts[i]?.$ref;
      const label = alt.title ?? ((alt.required ?? []).join(" + ") || `shape ${i + 1}`);
      const inner = ref && trail.includes(ref) ? asAbove(defName(rawAlts[i]), false) : children(root, alt, depth + 1, ref ? [...trail, ref] : trail);
      const desc = alt.description ? `<p class="pr-field-desc">${prose(alt.description)}</p>` : "";
      return `<li class="pr-field"><details${depth < 2 ? " open" : ""}><summary class="pr-field-head"><span class="pr-field-shape">${escape(label)}</span></summary>${desc}${inner}</details></li>`;
    });
    return `<ul class="pr-fields">${shapes.join("")}</ul>`;
  }
  if (!obj.properties) return "";
  const required = new Set(obj.required ?? []);
  const rows = Object.entries(obj.properties).map(([name, raw]) => field(root, name, raw, required.has(name), depth + 1, trail)).join("");
  // an object that takes one of several shapes says which keys choose between them
  const choice = (obj.oneOf ?? obj.anyOf)?.map((alt) => (alt.required ?? []).map((k) => `<code>${escape(k)}</code>`).join(" + ")).filter(Boolean);
  const note = choice?.length ? `<li class="pr-field-note">One of: ${choice.join(" or ")}</li>` : "";
  return `<ul class="pr-fields">${note}${rows}</ul>`;
}

function field(root, name, raw, required, depth, trail) {
  const s = resolve(root, raw);
  const ref = raw?.$ref;
  const type = typeOf(root, s, raw);
  const rules = constraints(s);
  const inner = ref && trail.includes(ref) ? asAbove(defName(raw), false) : children(root, s, depth, ref ? [...trail, ref] : trail);
  const head = `<span class="pr-field-name">${escape(name)}</span><span class="pr-field-type${type === "file" || type === "file[]" ? " is-file" : ""}">${escape(type)}</span>${required ? `<span class="pr-field-required">required</span>` : ""}${s.title && s.title !== name ? `<span class="pr-field-title">${escape(s.title)}</span>` : ""}`;
  const body = `${s.description ? `<p class="pr-field-desc">${prose(s.description)}</p>` : ""}${rules.length ? `<p class="pr-field-rules">${rules.join(" · ")}</p>` : ""}`;
  if (!inner) return `<li class="pr-field"><div class="pr-field-head">${head}</div>${body}</li>`;
  // nested fields fold, open near the top and closed deeper down
  return `<li class="pr-field"><details${depth < 3 ? " open" : ""}><summary class="pr-field-head">${head}</summary>${body}${inner}</details></li>`;
}

function schemaPanel(schema) {
  const root = schema;
  const top = resolve(root, schema);
  const intro = top.description ? `<p class="pr-field-desc">${prose(top.description)}</p>` : "";
  return `${intro}${children(root, top, 0, []) || '<p class="pr-field-desc">Any JSON value.</p>'}`;
}

function manifestPanel(m) {
  const rows = [];
  const row = (key, value) => value !== undefined && value !== null && value !== "" && rows.push(`<tr><th><code>${escape(key)}</code></th><td>${value}</td></tr>`);
  row("name", `<code>${escape(m.name)}</code>`);
  row("version", escape(m.version));
  row("title", escape(m.title ?? ""));
  row("description", m.description ? prose(m.description) : "");
  row("use_when", m.use_when ? prose(m.use_when) : "");
  if (m.artifacts) {
    const a = m.artifacts;
    const limits = [a.max_size ? `up to ${size(a.max_size)} each` : null, a.max_count ? `${a.max_count} a review` : null].filter(Boolean).join(", ");
    row("artifacts", `${a.accept.map((k) => `<code>${escape(k)}</code>`).join(" ")}${limits ? ` · ${limits}` : ""}`);
  }
  row("entry", m.entry ? `<code>${escape(m.entry)}</code>` : "");
  // the keys as keycaps, the way the docs write them everywhere else
  if (m.shortcuts?.length) row("shortcuts", `<ul class="pr-shortcuts">${m.shortcuts.map((s) => `<li>${kbdHtml(s.keys)}<span>${escape(s.does)}</span></li>`).join("")}</ul>`);
  return `<table class="pr-manifest"><tbody>${rows.join("")}</tbody></table>`;
}

/** The figure as markdown nodes: its markup as raw HTML, and each JSON
 *  view a ```json block, so the site's code viewer draws it as every other
 *  JSON on the page, with its colours and its copy button. */
function contract(name, alt) {
  const dir = path.join(plugins, name);
  const manifest = read(dir, "manifest.json");
  const payload = document(dir, manifest.payload_schema);
  const decision = document(dir, manifest.decision_schema);
  const tabs = [
    ["manifest", "Manifest", "manifest.json", manifestPanel(manifest), manifest],
    ["payload", "Payload", "what the agent sends", schemaPanel(payload), payload],
    ["decision", "Decision", "what comes back", schemaPanel(decision), decision],
  ];
  // a plugin's own settings, the rows it gets in Settings › Plugins
  if (manifest.settings_schema) {
    const settings = document(dir, manifest.settings_schema);
    tabs.push(["settings", "Settings", "Settings › Plugins", schemaPanel(settings), settings]);
  }
  const id = `pr-contract-${name}`;
  const buttons = tabs
    .map(([key, label, hint], i) => `<button type="button" role="tab" id="${id}-${key}-tab" aria-controls="${id}-${key}" aria-selected="${i === 0}" tabindex="${i === 0 ? 0 : -1}" data-contract-tab="${key}">${label}<span class="pr-contract-hint">${escape(hint)}</span></button>`)
    .join("");
  const views = `<div class="pr-contract-views" role="group" aria-label="Show as"><button type="button" data-contract-show="fields" aria-pressed="true">Fields</button><button type="button" data-contract-show="json" aria-pressed="false">JSON</button></div>`;
  const html = (value) => ({ type: "html", value });
  // one wrapper that opts out of the article's spacing, bar and panels inside it
  const nodes = [html(`<figure class="pr-contract" data-contract aria-label="${escape(alt)}"><div class="not-content"><div class="pr-contract-bar"><div class="pr-contract-tabs" role="tablist" aria-label="${escape(alt)}">${buttons}</div>${views}</div>`)];
  tabs.forEach(([key, , , fields, raw], i) => {
    nodes.push(html(`<div role="tabpanel" id="${id}-${key}" aria-labelledby="${id}-${key}-tab" data-contract-panel="${key}"${i === 0 ? "" : " hidden"}><div data-contract-view="fields">${fields}`));
    // the build is a command to run, so it is a shell block with its copy button
    if (key === "manifest" && manifest.build?.command) {
      nodes.push(html(`<div class="pr-manifest-build"><code class="pr-manifest-key">build</code>`));
      nodes.push({ type: "code", lang: "bash", meta: 'frame="none"', value: manifest.build.command });
      nodes.push(html(`</div>`));
    }
    nodes.push(html(`</div><div class="pr-contract-json" data-contract-view="json" hidden>`));
    nodes.push({ type: "code", lang: "json", meta: null, value: JSON.stringify(raw, null, 2) });
    nodes.push(html(`</div></div>`));
  });
  nodes.push(html(`</div></figure>`));
  return nodes;
}

const isContract = (child) => child.type === "image" && child.url.startsWith("contract:");

export default function remarkContract() {
  return (tree) => {
    visit(tree, "paragraph", (node, index, parent) => {
      if (!parent) return;
      const found = node.children.filter(isContract);
      const rest = node.children.filter((c) => !isContract(c) && !(c.type === "text" && !c.value.trim()));
      if (found.length !== 1 || rest.length) return;
      const [image] = found;
      const nodes = contract(image.url.slice("contract:".length), image.alt ?? "");
      parent.children.splice(index, 1, ...nodes);
      return index + nodes.length;
    });
  };
}
