// `pinrail-plugin check [dir]`: what the app's inspect says of a folder,
// without the app. The manifest's shape is schemas/manifest.schema.json, the
// same file the core holds every manifest to; what a schema cannot say (files
// that must exist, $refs, settings and key combinations) is carried here in
// JavaScript. A test in the core runs both over the same folders and
// compares, so they cannot drift quietly.
//
// A problem costs the plugin its place: the app will not install it. A
// warning costs it a feature: the app installs it and says on its row what
// was dropped (settings, shortcuts, the markdown template).
import fs from "node:fs";
import path from "node:path";
import Ajv2020 from "ajv/dist/2020.js";

/** The manifest's JSON Schema, as the core reads it too. */
export const MANIFEST_SCHEMA = JSON.parse(fs.readFileSync(new URL("../schemas/manifest.schema.json", import.meta.url), "utf8"));
const validateManifest = new Ajv2020({ allErrors: true, strict: false }).compile(MANIFEST_SCHEMA);

/** Keys whose violation costs the plugin that feature, not its place. */
const FEATURES = ["settings_schema", "shortcuts", "decision_template", "example", "sample"];
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

  // the schema first: a violation outside the features refuses the plugin,
  // one inside them costs that feature, as the core decides
  const dropped = new Set();
  if (!validateManifest(manifest)) {
    const seen = new Set();
    for (const e of validateManifest.errors) {
      const pointer = e.instancePath.slice(1);
      const key = pointer.split("/")[0] || e.params?.missingProperty || "manifest";
      if (seen.has(key)) continue;
      seen.add(key);
      const message = pointer ? `${pointer}: ${e.message}` : `${e.message}`;
      if (FEATURES.includes(key)) {
        dropped.add(key);
        warn(key, message);
      } else problem(key, message);
    }
  }
  const refused = (key) => problems.some((p) => p.key === key);

  if (!refused("name")) name = manifest.name;
  const v = versionOf(manifest.version);
  if (v && !refused("version")) ({ release, major } = v);
  if (manifest.title === undefined) warn("title", "no title: the app shows the name");

  const buildCommand = isObject(manifest.build) && !refused("build") ? manifest.build.command.trim() : null;

  if (!refused("entry")) entry = manifest.entry ?? "index.html";
  if (entry !== null) {
    const file = safeJoin(dir, entry);
    if (!file || !fs.existsSync(file) || !fs.statSync(file).isFile()) {
      if (buildCommand) notes.push({ key: "entry", message: `entry ${entry} not found yet: the build (${buildCommand}) has to write it` });
      else problem("entry", `entry ${entry} not found`);
    }
  }

  for (const key of ["payload_schema", "decision_schema"]) {
    if (!(key in manifest) || refused(key)) continue;
    const why = schemaProblem(dir, key, manifest[key]);
    if (why) problem(key, why);
  }

  if (manifest.settings_schema !== undefined && manifest.settings_schema !== null && !dropped.has("settings_schema")) {
    const why = settingsProblem(dir, manifest.settings_schema);
    if (why) warn("settings_schema", why);
  }

  if (manifest.shortcuts !== undefined && manifest.shortcuts !== null && !dropped.has("shortcuts")) {
    const why = shortcutsProblem(manifest.shortcuts);
    if (why) warn("shortcuts", why);
  }

  // the example must pass the plugin's own payload schema, as the app checks
  if (typeof manifest.example === "string" && !dropped.has("example") && !refused("payload_schema")) {
    const why = exampleProblem(dir, manifest.example, manifest.payload_schema);
    if (why) warn("example", why);
  }

  // the sample: a request with a title, a payload that passes, and its files
  if (typeof manifest.sample === "string" && !dropped.has("sample") && !refused("payload_schema")) {
    const why = sampleProblem(dir, manifest.sample, manifest.payload_schema);
    if (why) warn("sample", why);
  }

  // files beside the payload: each kind as the core reads it, and a payload
  // schema that says where they go, or an agent cannot tell
  const takesFiles = isObject(manifest.attachments) && !refused("attachments");
  if (takesFiles) {
    const bad = manifest.attachments.accept.find((k) => !isKind(k));
    if (bad !== undefined) problem("attachments", `attachments.accept: ${JSON.stringify(bad)} is neither an extension like .glb nor a media type like image/png`);
  }
  if (!refused("payload_schema") && "payload_schema" in manifest) {
    let names = false;
    try {
      names = JSON.stringify(schemaDocument(dir, manifest.payload_schema) ?? {}).includes('"$attachment"');
    } catch {
      // an unreadable schema is reported above
    }
    if (takesFiles && !names) warn("attachments", 'payload_schema never names {"$attachment": …}: say where a file goes, or an agent cannot tell (Pinrail.ATTACHMENT_SCHEMA is the $defs entry)');
    if (!takesFiles && names) warn("attachments", 'payload_schema names {"$attachment": …} but the manifest declares no attachments: the app refuses every file for this plugin');
  }

  if (manifest.decision_template !== undefined && manifest.decision_template !== null && !dropped.has("decision_template")) {
    const t = manifest.decision_template;
    if (!fs.existsSync(path.join(dir, t)) || !fs.statSync(path.join(dir, t)).isFile()) {
      warn("decision_template", `${t}: cannot read`);
    } else {
      notes.push({ key: "decision_template", message: `${t}: the app compiles it on install; render a decided fixture to see it` });
    }
  }

  return result();
}

/** `.ext`, `type/subtype` or `type/*`: a kind the core reads in attachments.accept. */
function isKind(kind) {
  const token = (t) => /^[A-Za-z0-9+.-]+$/.test(t);
  if (kind.startsWith(".")) return token(kind.slice(1));
  const [type, sub, ...rest] = kind.split("/");
  return rest.length === 0 && sub !== undefined && token(type) && (sub === "*" || token(sub));
}

/** The payload schema as a document: inline, or the file its $ref names. */
function schemaDocument(dir, schema) {
  if (!isObject(schema) || typeof schema.$ref !== "string") return schema;
  return JSON.parse(fs.readFileSync(safeJoin(dir, schema.$ref), "utf8"));
}

/** Why the manifest's example would be dropped, or null. Mirrors the core. */
function exampleProblem(dir, file, payloadSchema) {
  const at = safeJoin(dir, file);
  let payload;
  try {
    payload = JSON.parse(fs.readFileSync(at, "utf8"));
  } catch (e) {
    return e.code ? `${file}: cannot read` : `${file}: not JSON (${e.message})`;
  }
  let validate;
  try {
    const doc = { ...schemaDocument(dir, payloadSchema) };
    delete doc.$schema;
    delete doc.$id;
    validate = new Ajv2020({ allErrors: false, strict: false, validateFormats: false }).compile(doc);
  } catch {
    return null; // a schema this cannot compile is the payload_schema's problem, not the example's
  }
  if (validate(payload)) return null;
  const e = validate.errors[0];
  return `${file}: does not pass payload_schema at ${e.instancePath || "/"}: ${e.message}`;
}

/** Why the sample would be dropped, or null. Mirrors `plugins/sample.rs`. */
function sampleProblem(dir, file, payloadSchema) {
  const at = safeJoin(dir, file);
  if (!at) return `${file}: must stay inside the plugin's folder`;
  let request;
  try {
    request = JSON.parse(fs.readFileSync(at, "utf8"));
  } catch (e) {
    return e.code ? `${file}: cannot read` : `${file}: not JSON (${e.message})`;
  }
  if (typeof request.title !== "string" || !request.title.trim()) return `${file}: needs a title`;
  if (!isObject(request.payload)) return `${file}: needs a payload, a JSON object`;
  try {
    const doc = { ...schemaDocument(dir, payloadSchema) };
    delete doc.$schema;
    delete doc.$id;
    const validate = new Ajv2020({ allErrors: false, strict: false, validateFormats: false }).compile(doc);
    if (!validate(request.payload)) {
      const e = validate.errors[0];
      return `${file}: the payload does not pass payload_schema at ${e.instancePath || "/"}: ${e.message}`;
    }
  } catch {
    // a schema this cannot compile is the payload_schema's problem
  }
  const files = request.attachments;
  if (files === undefined || files === null) return null;
  if (!isObject(files)) return `${file}: attachments must map each name to a file`;
  const base = path.dirname(at);
  for (const [name, entry] of Object.entries(files)) {
    const relative = typeof entry === "string" ? entry : isObject(entry) ? entry.path : undefined;
    if (isObject(entry) && typeof relative !== "string") return `${file}: attachments.${name} needs a path`;
    if (typeof relative !== "string") return `${file}: attachments.${name} must be a path or {path, media_type}`;
    const where = safeJoin(base, relative);
    if (!where) return `${file}: attachments.${name} must stay inside the plugin's folder`;
    if (!fs.existsSync(where) || !fs.statSync(where).isFile()) return `${file}: attachments.${name}: ${relative} not found`;
  }
  return null;
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
