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
