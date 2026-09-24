/**
 * Files a fixture carries, for the harness and the dev shell to hand a
 * view the way the app does. A fixture lists them by name, with a path
 * relative to the fixture file:
 *
 *   "artifacts": { "pivot.glb": { "path": "pivot.glb" } }
 *
 * and the gate gets what the app would put there: name, size, media type
 * and hash for each, in name order.
 */
const crypto = require("node:crypto");
const fs = require("node:fs");
const path = require("node:path");

const TYPES = {
  ".glb": "model/gltf-binary", ".gltf": "model/gltf+json", ".png": "image/png", ".jpg": "image/jpeg",
  ".jpeg": "image/jpeg", ".gif": "image/gif", ".webp": "image/webp", ".svg": "image/svg+xml",
  ".pdf": "application/pdf", ".mp4": "video/mp4", ".webm": "video/webm", ".mp3": "audio/mpeg",
  ".wav": "audio/wav", ".ogg": "audio/ogg", ".oga": "audio/ogg", ".opus": "audio/ogg", ".m4a": "audio/mp4",
  ".aac": "audio/aac", ".flac": "audio/flac", ".json": "application/json", ".csv": "text/csv", ".txt": "text/plain",
};

/** `{ name: {path, media_type?} | path }` → `{ list: [...], files: { name: { path, media_type } } }` */
function resolveArtifacts(spec, baseDir) {
  const files = {};
  for (const [name, entry] of Object.entries(spec || {})) {
    const given = typeof entry === "string" ? { path: entry } : entry || {};
    if (typeof given.path !== "string") throw new Error(`artifact ${name}: give it a path`);
    const file = path.resolve(baseDir, given.path);
    if (!fs.existsSync(file)) throw new Error(`artifact ${name}: no file at ${file}`);
    files[name] = { path: file, media_type: given.media_type || TYPES[path.extname(name).toLowerCase()] || "application/octet-stream" };
  }
  const list = Object.keys(files)
    .sort()
    .map((name) => {
      const bytes = fs.readFileSync(files[name].path);
      return { name, size: bytes.length, media_type: files[name].media_type, sha256: crypto.createHash("sha256").update(bytes).digest("hex") };
    });
  return { list, files };
}

module.exports = { resolveArtifacts };
