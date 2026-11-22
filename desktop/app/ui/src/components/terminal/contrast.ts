// A floor on the contrast of a terminal's text against its background.
// Agents choose their colours for a dark background, so on a light one
// some nearly vanish, such as a white bullet or a light green. A colour
// below the floor moves toward black or white, whichever reads better,
// just far enough to reach it; every other colour is kept. The floor is
// low enough that faint text stays fainter than the rest.

/** The contrast ratio every text colour keeps against its background. */
export const MIN_CONTRAST = 3;

export type Rgb = { r: number; g: number; b: number };

/** The relative luminance of a colour, as WCAG defines it. */
function luminance({ r, g, b }: Rgb): number {
  const channel = (c: number) => {
    const s = c / 255;
    return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b);
}

/** The contrast ratio of two colours, from 1 to 21. */
export function contrast(a: Rgb, b: Rgb): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

const mix = (from: Rgb, to: Rgb, t: number): Rgb => ({
  r: Math.round(from.r + (to.r - from.r) * t),
  g: Math.round(from.g + (to.g - from.g) * t),
  b: Math.round(from.b + (to.b - from.b) * t),
});

const BLACK: Rgb = { r: 0, g: 0, b: 0 };
const WHITE: Rgb = { r: 255, g: 255, b: 255 };

/** `fg`, or the nearest colour toward black or white that keeps `min`
 *  contrast against `bg`. */
export function legible(fg: Rgb, bg: Rgb, min = MIN_CONTRAST): Rgb {
  if (contrast(fg, bg) >= min) return fg;
  // toward the end that stands further from the background
  const target = contrast(BLACK, bg) >= contrast(WHITE, bg) ? BLACK : WHITE;
  let lo = 0;
  let hi = 1;
  for (let i = 0; i < 12; i++) {
    const t = (lo + hi) / 2;
    if (contrast(mix(fg, target, t), bg) >= min) hi = t;
    else lo = t;
  }
  return mix(fg, target, hi);
}
