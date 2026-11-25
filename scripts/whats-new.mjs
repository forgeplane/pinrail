// The app ships What's new notes for the last few releases only. Each
// release that has notes has whats-new/<version>.json and a folder of
// pictures next to it. set-version.mjs calls `prune` when a release is
// made, and the release commit removes the oldest.

import fs from "node:fs";
import path from "node:path";

/** The number of releases whose notes the app ships. */
export const KEEP = 3;

const compare = (a, b) => {
  const pa = a.split(/[.-]/).map(Number);
  const pb = b.split(/[.-]/).map(Number);
  for (let i = 0; i < 3; i++) if ((pa[i] || 0) !== (pb[i] || 0)) return (pa[i] || 0) - (pb[i] || 0);
  return 0;
};

export const notesDir = (root) => path.join(root, "desktop", "app", "ui", "src", "whats-new");

/**
 * The releases up to `version`, the app's own, that have notes. Their
 * pictures show the app as it was released, and stay as they are.
 */
export function released(root, version) {
  return fs
    .readdirSync(notesDir(root))
    .filter((f) => /^\d+\.\d+\.\d+\.json$/.test(f))
    .map((f) => f.slice(0, -".json".length))
    .filter((v) => compare(v, version) <= 0);
}

/**
 * Removes the notes and pictures of every release but the newest `keep`
 * up to `version`, and returns the paths it removed, relative to `root`.
 * Notes for a release after `version` are left alone.
 */
export function prune(root, version, keep = KEEP) {
  const dir = notesDir(root);
  const versions = fs
    .readdirSync(dir)
    .filter((f) => /^\d+\.\d+\.\d+\.json$/.test(f))
    .map((f) => f.slice(0, -".json".length))
    .filter((v) => compare(v, version) <= 0)
    .sort(compare)
    .reverse();
  const removed = [];
  for (const old of versions.slice(keep)) {
    const notes = path.join(dir, `${old}.json`);
    const pictures = path.join(dir, old);
    fs.rmSync(notes);
    removed.push(path.relative(root, notes));
    if (fs.existsSync(pictures)) {
      for (const file of fs.readdirSync(pictures)) removed.push(path.relative(root, path.join(pictures, file)));
      fs.rmSync(pictures, { recursive: true });
    }
  }
  return removed;
}
