// `wicket-plugin check [dir]`: what the app's inspect says of a folder,
// without the app. The rules are the core's (desktop/core/src/plugins.rs),
// carried here in JavaScript; a test in the core runs both over the same
// folders and compares, so they cannot drift quietly.
//
// A problem costs the plugin its place: the app will not install it. A
// warning costs it a feature: the app installs it and says on its row what
// was dropped (settings, shortcuts, the markdown template).
import fs from "node:fs";
import path from "node:path";

const NAME = /^[a-z][a-z0-9_]*$/;
const ICON = /^[a-z0-9]+(-[a-z0-9]+)*$/;
const SCALARS = ["boolean", "string", "integer", "number"];
const MODIFIERS = ["cmd", "command", "super", "meta", "ctrl", "control", "alt", "option", "shift", "cmdorctrl", "commandorcontrol"];
const JSON_TYPES = ["null", "boolean", "object", "array", "number", "string", "integer"];

/** A manifest's version as text and its major, or null. Mirrors `version_of`. */
export function versionOf(value) {
  if (typeof value === "number") {
    if (!Number.isInteger(value) || value <= 0) return null;
    return { release: `${value}.0.0`, major: value };
  }
  if (typeof value === "string") {
    const parts = value.trim().split(".");
    if (parts.length !== 3 || parts.some((p) => !/^[0-9]+$/.test(p))) return null;
    return { release: value.trim(), major: Number(parts[0]) };
  }
  return null;
}

/** `dir/relative` when the path stays inside `dir`, else null. Mirrors `safe_join`. */
export function safeJoin(dir, relative) {
  const parts = relative.split(/[\\/]+/).filter((p) => p !== "" && p !== ".");
  if (parts.some((p) => p === "..") || path.isAbsolute(relative)) return null;
  return path.join(dir, ...parts);
}

const isObject = (v) => v !== null && typeof v === "object" && !Array.isArray(v);
const fits = (kind, value) =>
  kind === "boolean" ? typeof value === "boolean"
  : kind === "string" ? typeof value === "string"
  : kind === "integer" ? Number.isInteger(value)
  : kind === "number" ? typeof value === "number"
  : false;

/**
 * Checks the plugin at `dir`. Returns `{ ok, usable, name, release, major,
 * entry, problems, warnings, notes }`: `ok` when the app would install it,
 * `usable` when it would also serve it as it is (a source that builds is
 * `ok` before its build and `usable` after).
 */
export function checkPlugin(dir) {
  dir = path.resolve(dir);
  const problems = [];
  const warnings = [];
  const notes = [];
  const problem = (key, message) => problems.push({ key, message });
  const warn = (key, message) => warnings.push({ key, message });
  const result = () => ({
    dir,
    ok: problems.length === 0,
    usable: problems.length === 0 && !notes.some((n) => n.key === "entry"),
    name, release, major, entry, problems, warnings, notes,
  });

  let name = null, release = null, major = null, entry = null;
  let manifest;
  try {
    manifest = JSON.parse(fs.readFileSync(path.join(dir, "manifest.json"), "utf8"));
  } catch (e) {
    problem("manifest", e.code === "ENOENT" ? "no manifest.json" : `manifest.json is not valid JSON (${e.message})`);
    return result();
  }
  if (!isObject(manifest)) {
    problem("manifest", "manifest.json must be a JSON object");
    return result();
  }

  if (typeof manifest.name !== "string") problem("name", "name is required, a string");
  else if (!NAME.test(manifest.name)) problem("name", `name ${JSON.stringify(manifest.name)} is not valid: [a-z][a-z0-9_]*`);
  else name = manifest.name;

  const v = versionOf(manifest.version);
  if (!v || v.release === "0.0.0") problem("version", 'version is required: a positive integer, or a semantic version like "1.2.0"');
  else ({ release, major } = v);

  if (manifest.title === undefined) warn("title", "no title: the app shows the name");
  else if (typeof manifest.title !== "string") warn("title", "title must be a string; the app shows the name");

  const build = manifest.build;
  let buildCommand = null;
  if (build !== undefined && build !== null) {
    if (isObject(build) && typeof build.command === "string" && build.command.trim() !== "") buildCommand = build.command.trim();
    else problem("build", 'build must be { "command": "…" }, the command that writes the bundle');
  }

  if (manifest.entry === undefined) entry = "index.html";
  else if (typeof manifest.entry === "string" && manifest.entry !== "" && !manifest.entry.startsWith("/") && !manifest.entry.includes("\0")) entry = manifest.entry;
  else problem("entry", `entry ${JSON.stringify(manifest.entry)} is not valid: a path inside the folder, like "view/index.html"`);
  if (entry !== null) {
    const file = safeJoin(dir, entry);
    if (!file || !fs.existsSync(file) || !fs.statSync(file).isFile()) {
      if (buildCommand) notes.push({ key: "entry", message: `entry ${entry} not found yet: the build (${buildCommand}) has to write it` });
      else problem("entry", `entry ${entry} not found`);
    }
  }

  for (const key of ["payload_schema", "decision_schema"]) {
    if (!(key in manifest)) {
      problem(key, `${key} is required`);
      continue;
    }
    const why = schemaProblem(dir, key, manifest[key]);
    if (why) problem(key, why);
  }

  if (manifest.icon !== undefined && manifest.icon !== null) {
    if (typeof manifest.icon !== "string" || !ICON.test(manifest.icon)) {
      problem("icon", `icon ${JSON.stringify(manifest.icon)} is not valid: a lucide icon name, like "mail" or "git-pull-request"`);
    }
  }

  if (manifest.min_height !== undefined && manifest.min_height !== null) {
    if (!Number.isInteger(manifest.min_height) || manifest.min_height <= 0) warn("min_height", "min_height must be a positive integer; the app uses 400");
  }

  if (manifest.settings_schema !== undefined && manifest.settings_schema !== null) {
    const why = settingsProblem(dir, manifest.settings_schema);
    if (why) warn("settings_schema", why);
  }

  if (manifest.shortcuts !== undefined && manifest.shortcuts !== null) {
    const why = shortcutsProblem(manifest.shortcuts);
    if (why) warn("shortcuts", why);
  }

  if (manifest.decision_template !== undefined && manifest.decision_template !== null) {
    const t = manifest.decision_template;
    if (typeof t !== "string" || t === "" || t.includes("..") || t.startsWith("/")) {
      warn("decision_template", "decision_template must name a file beside the manifest");
    } else if (!fs.existsSync(path.join(dir, t)) || !fs.statSync(path.join(dir, t)).isFile()) {
      warn("decision_template", `${t}: cannot read`);
    } else {
      notes.push({ key: "decision_template", message: `${t}: the app compiles it on install; render a decided fixture to see it` });
    }
  }

  return result();
}

/** Why a payload or decision schema would be refused, or null. */
function schemaProblem(dir, key, schema) {
  if (!isObject(schema)) return `${key} must be a JSON Schema object`;
  if (typeof schema.$ref === "string") {
    const file = safeJoin(dir, schema.$ref);
    if (!file) return `${key}: $ref ${schema.$ref} leaves the plugin directory`;
    let text;
    try {
      text = fs.readFileSync(file, "utf8");
    } catch (e) {
      return `${key}: $ref ${schema.$ref} cannot be read (${e.code ?? e.message})`;
    }
    let doc;
    try {
      doc = JSON.parse(text);
    } catch (e) {
      return `${key}: $ref ${schema.$ref} is not valid JSON (${e.message})`;
    }
    if (!isObject(doc)) return `${key}: ${schema.$ref} must be a JSON Schema object`;
    return typeProblem(key, doc);
  }
  return typeProblem(key, schema);
}

function typeProblem(key, schema) {
  const t = schema.type;
  if (t === undefined) return null;
  const types = Array.isArray(t) ? t : [t];
  const bad = types.find((x) => !JSON_TYPES.includes(x));
  return bad === undefined ? null : `${key}: type ${JSON.stringify(bad)} is not a JSON Schema type`;
}

/** Why a settings schema would be dropped, or null. Mirrors `settings::load`. */
function settingsProblem(dir, raw) {
  if (!isObject(raw)) return "settings_schema must be a JSON Schema object";
  let doc = raw;
  if ("$ref" in raw) {
    if (typeof raw.$ref !== "string") return "settings_schema $ref must be a relative path";
    const file = safeJoin(dir, raw.$ref);
    if (!file) return `settings_schema $ref ${raw.$ref} leaves the plugin directory`;
    let text;
    try {
      text = fs.readFileSync(file, "utf8");
    } catch (e) {
      return `settings_schema $ref ${raw.$ref} cannot be read (${e.code ?? e.message})`;
    }
    try {
      doc = JSON.parse(text);
    } catch (e) {
      return `settings_schema $ref ${raw.$ref} is not valid JSON (${e.message})`;
    }
    if (!isObject(doc)) return "settings_schema must be a JSON Schema object";
  }
  if (doc.type !== undefined && doc.type !== "object") return "settings_schema must describe an object";
  if (!isObject(doc.properties)) return "settings_schema must have properties";
  for (const [key, p] of Object.entries(doc.properties)) {
    if (!isObject(p)) return `settings_schema property ${key} must be an object`;
    const kind = p.type;
    if (!SCALARS.includes(kind)) return `settings_schema property ${key} must have a type of ${SCALARS.join(", ")}`;
    if (!("default" in p)) return `settings_schema property ${key} needs a default`;
    if (!fits(kind, p.default)) return `settings_schema property ${key}: the default is not a ${kind}`;
    if ("enum" in p) {
      const ok = Array.isArray(p.enum) && p.enum.length > 0 && p.enum.every((i) => fits(kind, i));
      if (!ok) return `settings_schema property ${key}: enum must list ${kind} values`;
    }
    if ("oneOf" in p) {
      const ok = Array.isArray(p.oneOf) && p.oneOf.length > 0 && p.oneOf.every((i) => isObject(i) && "const" in i && fits(kind, i.const));
      if (!ok) return `settings_schema property ${key}: oneOf must list {"const": …} ${kind} values`;
    }
  }
  return null;
}

/** Why a shortcuts list would be dropped, or null. Mirrors `shortcuts::load`. */
function shortcutsProblem(raw) {
  if (!Array.isArray(raw)) return "shortcuts must be a list of {keys, does}";
  for (const [i, item] of raw.entries()) {
    if (!isObject(item)) return `shortcuts[${i}] must be an object with keys and does`;
    if (typeof item.keys !== "string" || item.keys.trim() === "") return `shortcuts[${i}] needs keys, a string`;
    if (!normalizeKeys(item.keys)) return `shortcuts[${i}]: keys ${JSON.stringify(item.keys)} is not a key combination`;
    if (typeof item.does !== "string" || item.does.trim() === "") return `shortcuts[${i}] needs does, a string`;
    if (item.group !== undefined && item.group !== null) {
      if (typeof item.group !== "string" || item.group.trim() === "") return `shortcuts[${i}]: group must be a string`;
    }
  }
  return null;
}

function normalizeKeys(keys) {
  const parts = keys.split("+").map((p) => p.trim().toLowerCase());
  const key = parts[parts.length - 1];
  if (key === "" || /\s/.test(key)) return null;
  if (parts.slice(0, -1).some((m) => !MODIFIERS.includes(m))) return null;
  return parts.join("+");
}

/** The command line: `check [dir] [--json]`. Exits 1 when the app would refuse the folder. */
export function check(argv) {
  const args = [...argv];
  const json = args.includes("--json");
  const dir = args.find((a) => !a.startsWith("--")) ?? ".";
  const r = checkPlugin(dir);
  if (json) {
    console.log(JSON.stringify(r, null, 2));
  } else {
    const head = r.name ? `${r.name} ${r.release ?? ""}`.trim() : path.basename(r.dir);
    console.log(`${head} in ${path.relative(process.cwd(), r.dir) || "."}`);
    for (const p of r.problems) console.log(`  problem  ${p.message}`);
    for (const w of r.warnings) console.log(`  warning  ${w.message}`);
    for (const n of r.notes) console.log(`  note     ${n.message}`);
    if (r.ok) console.log(r.usable ? "  ok: the app would install and serve it" : "  ok: the app would build, then install it");
    else console.log("  the app would refuse it");
  }
  process.exit(r.ok ? 0 : 1);
}
