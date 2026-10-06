/**
 * The checks against a plugin's JSON Schemas that the harness and the dev
 * shell make, as the core makes them: draft 2020-12, every error reported,
 * formats not checked.
 */
const fs = require("node:fs");
const path = require("node:path");
const Ajv2020 = require("ajv/dist/2020").default;

/** A function that gives the violations of a value against `schema`, as
 *  `{ path, message }`, or none. */
function checker(schema) {
  const doc = { ...schema };
  delete doc.$schema;
  delete doc.$id;
  const validate = new Ajv2020({ allErrors: true, strict: false, validateFormats: false }).compile(doc);
  return (data) => (validate(data) ? [] : validate.errors.map((e) => ({ path: e.instancePath, message: e.message })));
}

/** The checker of a plugin's `schemas/<kind>.schema.json`, `kind` being
 *  "payload" or "decision"; a plugin without the file accepts anything. */
function schemaChecker(pluginDir, kind) {
  const file = path.join(pluginDir, "schemas", `${kind}.schema.json`);
  if (!fs.existsSync(file)) return () => [];
  return checker(JSON.parse(fs.readFileSync(file, "utf8")));
}

module.exports = { checker, schemaChecker };
