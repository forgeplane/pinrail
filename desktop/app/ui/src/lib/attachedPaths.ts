// The paths of files attached to a session's messages, as they appear in a
// prompt: each on a line of its own, quoted when a shell would need it, in
// the session's files folder under the data directory.

/** A line that is only the path of a file attached to a session. */
const ATTACHED_LINE = /^'?(\/.*\/sessions\/[^/]+\/[^/]+\/files\/[^/]+?)'?$/;

/** The placeholder an agent shows in a prompt for an image it attached. */
const IMAGE_PLACEHOLDER = /\[Image #\d+\]/g;

/** The path a line names, when the line is only an attached file's path. */
export function attachedPath(line: string): string | null {
  return ATTACHED_LINE.exec(line.trim())?.[1]?.replaceAll("'\\''", "'") ?? null;
}

/** A text without the lines that are attached files' paths, and without
 *  the `[Image #N]` an agent shows in place of an image it attached. */
export function withoutAttachments(text: string): string {
  return text
    .split("\n")
    .filter((line) => attachedPath(line) === null)
    .join("\n")
    .replace(IMAGE_PLACEHOLDER, "");
}

/** A file a text names by an attached path: the path, the session it was
 *  attached in, and its name. */
export type AttachedFile = { path: string; agent: string; session: string; name: string };

/** The file a path names, when it is in a session's files folder. */
function fileOf(path: string): AttachedFile | null {
  const match = /\/sessions\/([^/]+)\/([^/]+)\/files\/([^/]+)$/.exec(path);
  return match ? { path, agent: match[1], session: match[2], name: match[3] } : null;
}

/** Whether a file's name says it is a picture a page can show. */
export const isImage = (name: string) => /\.(png|jpe?g|gif|webp|bmp|svg)$/i.test(name);

/** A text split into what the person wrote, the files attached to it, and
 *  how many images the agent shows as `[Image #N]`. */
export function attachedParts(text: string): { text: string; files: AttachedFile[]; images: number } {
  const files: AttachedFile[] = [];
  const kept: string[] = [];
  for (const line of text.split("\n")) {
    const path = attachedPath(line);
    const file = path ? fileOf(path) : null;
    if (file) files.push(file);
    else kept.push(line);
  }
  const joined = kept.join("\n");
  const images = joined.match(IMAGE_PLACEHOLDER)?.length ?? 0;
  return { text: joined.replace(IMAGE_PLACEHOLDER, "").trim(), files, images };
}
