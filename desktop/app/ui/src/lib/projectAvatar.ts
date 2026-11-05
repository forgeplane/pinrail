// A project's avatar as data: the colour it shows and the mark on it, from
// what the person chose or, failing that, from the project itself. The
// ProjectAvatar component draws it.

/** The colours a project can take, as the core names them. */
export const PROJECT_COLORS = [
  "gray",
  "red",
  "orange",
  "amber",
  "green",
  "teal",
  "blue",
  "indigo",
  "violet",
  "pink",
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

/** What the avatar shows: the emoji, or the name's first letter. */
export const markOf = (project: Pick<Avatar, "name" | "emoji">) =>
  project.emoji || [...project.name.trim()][0]?.toUpperCase() || "?";
