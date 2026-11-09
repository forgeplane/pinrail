/**
 * Files a fixture carries, for the harness and the dev shell to hand a
 * view the way the app does. A fixture lists them by name, with a path
 * relative to the fixture file:
 *
 *   "attachments": { "pivot.glb": { "path": "pivot.glb" } }
 *
 * and the review gets what the app would put there: name, size, media type
 * and hash for each, in name order.
 */
const { createHash } = require("node:crypto");
const fs = require("node:fs");
const path = require("node:path");

// as the format crate names them, which test/attachments.test.cjs checks
const TYPES = {
  ".glb": "model/gltf-binary",
  ".gltf": "model/gltf+json",
  ".obj": "model/obj",
  ".stl": "model/stl",
  ".usdz": "model/vnd.usdz+zip",
  ".png": "image/png",
  ".jpg": "image/jpeg",
  ".jpeg": "image/jpeg",
  ".gif": "image/gif",
  ".webp": "image/webp",
  ".avif": "image/avif",
  ".svg": "image/svg+xml",
  ".pdf": "application/pdf",
  ".mp4": "video/mp4",
  ".webm": "video/webm",
  ".mov": "video/quicktime",
  ".mp3": "audio/mpeg",
  ".wav": "audio/wav",
  ".ogg": "audio/ogg",
  ".oga": "audio/ogg",
  ".opus": "audio/ogg",
  ".m4a": "audio/mp4",
  ".aac": "audio/aac",
  ".flac": "audio/flac",
  ".json": "application/json",
  ".csv": "text/csv",
  ".txt": "text/plain",
  ".md": "text/plain",
  ".log": "text/plain",
  ".html": "text/html",
  ".htm": "text/html",
  ".zip": "application/zip",
};

/** `{ name: {path, media_type?} | path }` → `{ list: [...], files: { name: { path, media_type } } }` */
function resolveAttachments(spec, baseDir) {
  const files = {};
  for (const [name, entry] of Object.entries(spec || {})) {
    const given = typeof entry === "string" ? { path: entry } : entry || {};
    if (typeof given.path !== "string") throw new Error(`attachment ${name}: give it a path`);
    const file = path.resolve(baseDir, given.path);
    if (!fs.existsSync(file)) throw new Error(`attachment ${name}: no file at ${file}`);
    files[name] = {
      path: file,
      media_type: given.media_type || TYPES[path.extname(name).toLowerCase()] || "application/octet-stream",
    };
  }
  const list = Object.keys(files)
    .sort()
    .map((name) => {
      const bytes = fs.readFileSync(files[name].path);
      return {
        name,
        size: bytes.length,
        media_type: files[name].media_type,
        sha256: createHash("sha256").update(bytes).digest("hex"),
      };
    });
  return { list, files };
}

module.exports = { resolveAttachments, TYPES };
