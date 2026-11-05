// A project's avatar as data: the colour it shows and the mark on it, from
// what the person chose or, failing that, from the project itself. The
// ProjectAvatar component draws it.

/** The colours a project can take, as the core names them, around the colour wheel and the neutrals last. */
export const PROJECT_COLORS = [
  "red",
  "orange",
  "amber",
  "lime",
  "green",
  "teal",
  "cyan",
  "blue",
  "indigo",
  "violet",
  "fuchsia",
  "pink",
  "rose",
  "brown",
  "slate",
  "gray",
] as const;
export type ProjectColor = (typeof PROJECT_COLORS)[number];

export type Avatar = { id: string; name: string; color?: string | null; emoji?: string | null };

/** The colour a project shows: its own, or one picked by its id. */
export function colorOf(project: Pick<Avatar, "id" | "color">): ProjectColor {
  if (project.color && (PROJECT_COLORS as readonly string[]).includes(project.color))
    return project.color as ProjectColor;
  let hash = 0;
  for (const c of project.id) hash = (hash * 31 + c.charCodeAt(0)) >>> 0;
  return PROJECT_COLORS[hash % PROJECT_COLORS.length];
}

/** The most letters or digits an avatar shows, as the core allows. */
export const MAX_LETTERS = 3;

/** Whether a mark is letters or digits, which sit on a baseline, rather than an emoji. */
export const isLetters = (mark: string) => /^[\p{L}\p{N}]+$/u.test(mark);

/** What the avatar shows: the emoji, the letters given in capitals, or the
 *  name's first letter. */
export function markOf(project: Pick<Avatar, "name" | "emoji">): string {
  const icon = project.emoji?.trim();
  if (icon) return isLetters(icon) ? icon.toUpperCase() : icon;
  return [...project.name.trim()][0]?.toUpperCase() || "?";
}

/** The letters' size in the avatar, as a share of its side: smaller for
 *  more of them, so that up to three fit in the square. */
export const letterSize = (mark: string) => [0.62, 0.62, 0.46, 0.34][Math.min([...mark].length, 3)];

/** What the icon field keeps of what is typed: letters or digits stop at
 *  three; an emoji is left whole. */
export const iconInput = (typed: string) =>
  isLetters(typed.trim()) ? [...typed.trim()].slice(0, MAX_LETTERS).join("") : typed;
