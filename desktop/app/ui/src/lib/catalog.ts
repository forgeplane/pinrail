// Searching the official plugins the app carries, in the field that also
// takes the path of a plugin's folder or zip.

import type { CatalogEntry } from "../api/types";

/** Text that names a folder or a zip, rather than words to search for. */
export const isPath = (text: string) =>
  /^[/~.\\]/.test(text) || /^[A-Za-z]:[\\/]/.test(text) || text.toLowerCase().endsWith(".zip");

/** The official plugins that match `query` by name, title or description;
 *  with no query, the ones not installed. */
export function matching(entries: CatalogEntry[], query: string): CatalogEntry[] {
  const q = query.trim().toLowerCase();
  if (!q) return entries.filter((e) => !e.installed);
  return entries.filter((e) =>
    [e.name, e.title, e.description ?? "", e.use_when ?? ""].some((text) => text.toLowerCase().includes(q)),
  );
}
