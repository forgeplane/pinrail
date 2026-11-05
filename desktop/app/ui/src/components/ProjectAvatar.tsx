// A project's avatar: a rounded square in the project's colour with its
// emoji, or the first letter of its name when it has none. A project without
// a colour takes one from the palette by its id, so each keeps its own.
//
// The mark is SVG text, placed by its baseline: a letter's baseline sits
// half the capital height below the middle, so the letter itself is
// centred, the same in every browser engine. HTML text would centre its
// line box, which each engine fills differently.

import type { CSSProperties } from "react";
import { type Avatar, colorOf, isLetters, letterSize, markOf } from "../lib/projectAvatar";

export { PROJECT_COLORS, colorOf, type ProjectColor } from "../lib/projectAvatar";

/** The height of the app font's capital letters, as a share of its size. */
const CAP = 0.727;

export function ProjectAvatar({
  project,
  size = 16,
  className = "",
}: {
  project: Avatar;
  size?: number;
  className?: string;
}) {
  const mark = markOf(project);
  return (
    <span
      className={`project-avatar ${isLetters(mark) ? "" : "is-emoji"} ${className}`}
      style={{ "--avatar": `var(--project-${colorOf(project)})`, "--icon": `${size}px` } as CSSProperties}
      data-color={colorOf(project)}
      aria-hidden
    >
      <svg className="project-avatar-mark" viewBox="0 0 100 100" width="100%" height="100%">
        {isLetters(mark) ? (
          // Inter's capitals are 0.727 of its size: half of it below the middle
          <text x="50" y="50" dy={`${CAP / 2}em`} textAnchor="middle" fontSize={letterSize(mark) * 100}>
            {mark}
          </text>
        ) : (
          <text x="50" y="50" dominantBaseline="central" textAnchor="middle" fontSize="70">
            {mark}
          </text>
        )}
      </svg>
    </span>
  );
}
