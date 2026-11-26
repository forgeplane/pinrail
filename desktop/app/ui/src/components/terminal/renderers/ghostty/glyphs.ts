// The glyphs the renderer draws itself, from the cell's own edges rather
// than the font, so that lines meet without gaps at any size: box drawing,
// block elements, Braille, the Powerline separators, and the ⏺ that Claude
// Code marks each step with, which few terminal fonts have.

/** What drawing a glyph needs of a canvas context. */
export interface Pen {
  fillStyle: string | CanvasGradient | CanvasPattern;
  strokeStyle: string | CanvasGradient | CanvasPattern;
  lineWidth: number;
  globalAlpha: number;
  fillRect(x: number, y: number, w: number, h: number): void;
  beginPath(): void;
  moveTo(x: number, y: number): void;
  lineTo(x: number, y: number): void;
  quadraticCurveTo(cx: number, cy: number, x: number, y: number): void;
  arc(x: number, y: number, r: number, start: number, end: number, anticlockwise?: boolean): void;
  closePath(): void;
  fill(): void;
  stroke(): void;
}

/** A cell's rectangle. */
export interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}

/** How heavy an arm of a box-drawing glyph is: none, light (1), heavy or
 *  double. */
const NONE = 0;
const HEAVY = 2;
const DOUBLE = 3;

/**
 * Each box-drawing character from U+2500 as the weights of its arms up,
 * right, down and left, with the dashes of a dashed line after a slash.
 * A null is drawn otherwise: the rounded corners and the diagonals.
 */
const BOX: (string | null)[] = [
  // ─ ━ │ ┃ ┄ ┅ ┆ ┇ ┈ ┉ ┊ ┋
  "0101",
  "0202",
  "1010",
  "2020",
  "0101/3",
  "0202/3",
  "1010/3",
  "2020/3",
  "0101/4",
  "0202/4",
  "1010/4",
  "2020/4",
  // ┌ ┍ ┎ ┏ ┐ ┑ ┒ ┓ └ ┕ ┖ ┗ ┘ ┙ ┚ ┛
  "0110",
  "0210",
  "0120",
  "0220",
  "0011",
  "0012",
  "0021",
  "0022",
  "1100",
  "1200",
  "2100",
  "2200",
  "1001",
  "1002",
  "2001",
  "2002",
  // ├ ┝ ┞ ┟ ┠ ┡ ┢ ┣ ┤ ┥ ┦ ┧ ┨ ┩ ┪ ┫
  "1110",
  "1210",
  "2110",
  "1120",
  "2120",
  "2210",
  "1220",
  "2220",
  "1011",
  "1012",
  "2011",
  "1021",
  "2021",
  "2012",
  "1022",
  "2022",
  // ┬ ┭ ┮ ┯ ┰ ┱ ┲ ┳ ┴ ┵ ┶ ┷ ┸ ┹ ┺ ┻
  "0111",
  "0112",
  "0211",
  "0212",
  "0121",
  "0122",
  "0221",
  "0222",
  "1101",
  "1102",
  "1201",
  "1202",
  "2101",
  "2102",
  "2201",
  "2202",
  // ┼ ┽ ┾ ┿ ╀ ╁ ╂ ╃ ╄ ╅ ╆ ╇ ╈ ╉ ╊ ╋
  "1111",
  "1112",
  "1211",
  "1212",
  "2111",
  "1121",
  "2121",
  "2112",
  "2211",
  "1122",
  "1221",
  "2212",
  "1222",
  "2122",
  "2221",
  "2222",
  // ╌ ╍ ╎ ╏
  "0101/2",
  "0202/2",
  "1010/2",
  "2020/2",
  // ═ ║ ╒ ╓ ╔ ╕ ╖ ╗ ╘ ╙ ╚ ╛ ╜ ╝
  "0303",
  "3030",
  "0310",
  "0130",
  "0330",
  "0013",
  "0031",
  "0033",
  "1300",
  "3100",
  "3300",
  "1003",
  "3001",
  "3003",
  // ╞ ╟ ╠ ╡ ╢ ╣ ╤ ╥ ╦ ╧ ╨ ╩ ╪ ╫ ╬
  "1310",
  "3130",
  "3330",
  "1013",
  "3031",
  "3033",
  "0313",
  "0131",
  "0333",
  "1303",
  "3101",
  "3303",
  "1313",
  "3131",
  "3333",
  // ╭ ╮ ╯ ╰ ╱ ╲ ╳
  null,
  null,
  null,
  null,
  null,
  null,
  null,
  // ╴ ╵ ╶ ╷ ╸ ╹ ╺ ╻ ╼ ╽ ╾ ╿
  "0001",
  "1000",
  "0100",
  "0010",
  "0002",
  "2000",
  "0200",
  "0020",
  "0201",
  "1020",
  "0102",
  "2010",
];

const BOX_FIRST = 0x2500;
const BLOCK_FIRST = 0x2580;
const BRAILLE_FIRST = 0x2800;
const POWERLINE = new Set([0xe0b0, 0xe0b1, 0xe0b2, 0xe0b3, 0xe0b4, 0xe0b5, 0xe0b6, 0xe0b7]);
/** ⏺, BLACK CIRCLE FOR RECORD. */
const RECORD = 0x23fa;

/** The code point of a one-character cell, or null. */
function single(text: string): number | null {
  const cp = text.codePointAt(0);
  // one code point: one UTF-16 unit, or two for one past U+FFFF
  if (cp === undefined || text.length !== (cp > 0xffff ? 2 : 1)) return null;
  return cp;
}

/** Whether the renderer draws `text` itself. */
export function isDrawn(text: string): boolean {
  const cp = single(text);
  if (cp === null) return false;
  return (
    (cp >= BOX_FIRST && cp < BOX_FIRST + BOX.length) ||
    (cp >= BLOCK_FIRST && cp <= 0x259f) ||
    (cp >= BRAILLE_FIRST && cp <= 0x28ff) ||
    POWERLINE.has(cp) ||
    cp === RECORD
  );
}

/** The thickness of a light line in a cell, at least one pixel. */
export function lightWidth(box: Box): number {
  return Math.max(1, Math.round(Math.min(box.w, box.h) / 10));
}

/** Draws `text` in `box`, in the pen's fill style. False when it is not a
 *  glyph the renderer draws. */
export function drawGlyph(pen: Pen, text: string, box: Box): boolean {
  const cp = single(text);
  if (cp === null) return false;
  if (cp >= BOX_FIRST && cp < BOX_FIRST + BOX.length) {
    const arms = BOX[cp - BOX_FIRST];
    if (arms) drawArms(pen, arms, box);
    else drawCurve(pen, cp, box);
    return true;
  }
  if (cp >= BLOCK_FIRST && cp <= 0x259f) return drawBlock(pen, cp, box);
  if (cp >= BRAILLE_FIRST && cp <= 0x28ff) {
    drawBraille(pen, cp - BRAILLE_FIRST, box);
    return true;
  }
  if (POWERLINE.has(cp)) {
    drawPowerline(pen, cp, box);
    return true;
  }
  if (cp === RECORD) {
    drawRecord(pen, box);
    return true;
  }
  return false;
}

/** ⏺ as a filled circle in the middle of the cell, 62% of the cell's
 *  width across, or half its height in a wide cell. */
function drawRecord(pen: Pen, { x, y, w, h }: Box) {
  const d = Math.min(w * 0.62, h * 0.5);
  pen.beginPath();
  pen.arc(x + w / 2, y + h / 2, d / 2, 0, Math.PI * 2);
  pen.fill();
}

/** The arms of a box-drawing glyph, from the centre to each edge. */
function drawArms(pen: Pen, spec: string, { x, y, w, h }: Box) {
  const [weights, dashes] = spec.split("/");
  const [up, right, down, left] = [...weights].map(Number);
  const light = lightWidth({ x, y, w, h });
  const thick = (weight: number) => (weight === HEAVY ? light * 2 : light);
  const cx = Math.round(x + w / 2);
  const cy = Math.round(y + h / 2);
  // a double line is two light lines a light line apart
  const gap = light;
  const across = (weight: number, from: number, to: number, vertical: boolean) => {
    if (weight === NONE) return;
    const lines = weight === DOUBLE ? [-gap - light / 2, gap + light / 2] : [0];
    const t = thick(weight);
    for (const offset of lines) {
      const segment = (a: number, b: number) =>
        vertical
          ? pen.fillRect(Math.round(cx + offset - t / 2), Math.min(a, b), t, Math.abs(b - a))
          : pen.fillRect(Math.min(a, b), Math.round(cy + offset - t / 2), Math.abs(b - a), t);
      if (!dashes) {
        segment(from, to);
        continue;
      }
      // a dashed line: as many dashes as it says, with gaps between
      const n = Number(dashes);
      const length = (to - from) / n;
      for (let i = 0; i < n; i++) segment(from + i * length, from + i * length + length * 0.6);
    }
  };
  if (dashes) {
    across(up || down, y, y + h, true);
    across(left || right, x, x + w, false);
    return;
  }
  // an arm ends at the far edge of the line it crosses, in whole pixels,
  // so arms meet without a gap and without a stub; with no line to cross,
  // at the centre
  const across_ = (a: number, b: number) => Math.max(a ? thick(a) : 0, b ? thick(b) : 0);
  const vThick = across_(up, down);
  const hThick = across_(left, right);
  const hTop = Math.round(cy - hThick / 2);
  const vLeft = Math.round(cx - vThick / 2);
  across(up, y, hThick ? hTop + hThick : cy, true);
  across(down, hThick ? hTop : cy, y + h, true);
  across(left, x, vThick ? vLeft + vThick : cx, false);
  across(right, vThick ? vLeft : cx, x + w, false);
}

/** The rounded corners and the diagonals. */
function drawCurve(pen: Pen, cp: number, { x, y, w, h }: Box) {
  const cx = x + w / 2;
  const cy = y + h / 2;
  pen.lineWidth = lightWidth({ x, y, w, h });
  pen.strokeStyle = pen.fillStyle;
  pen.beginPath();
  switch (cp) {
    case 0x256d: // ╭
      pen.moveTo(cx, y + h);
      pen.quadraticCurveTo(cx, cy, x + w, cy);
      break;
    case 0x256e: // ╮
      pen.moveTo(cx, y + h);
      pen.quadraticCurveTo(cx, cy, x, cy);
      break;
    case 0x256f: // ╯
      pen.moveTo(cx, y);
      pen.quadraticCurveTo(cx, cy, x, cy);
      break;
    case 0x2570: // ╰
      pen.moveTo(cx, y);
      pen.quadraticCurveTo(cx, cy, x + w, cy);
      break;
    case 0x2571: // ╱
      pen.moveTo(x + w, y);
      pen.lineTo(x, y + h);
      break;
    case 0x2572: // ╲
      pen.moveTo(x, y);
      pen.lineTo(x + w, y + h);
      break;
    case 0x2573: // ╳
      pen.moveTo(x + w, y);
      pen.lineTo(x, y + h);
      pen.moveTo(x, y);
      pen.lineTo(x + w, y + h);
      break;
  }
  pen.stroke();
}

/** Block elements: eighths, halves, quadrants and shades. */
function drawBlock(pen: Pen, cp: number, { x, y, w, h }: Box): boolean {
  const rect = (fx: number, fy: number, fw: number, fh: number) =>
    pen.fillRect(Math.round(x + fx * w), Math.round(y + fy * h), Math.round(fw * w), Math.round(fh * h));
  if (cp === 0x2580)
    rect(0, 0, 1, 0.5); // ▀
  else if (cp >= 0x2581 && cp <= 0x2588) {
    // ▁ to █: the lower eighths
    const eighths = cp - 0x2580;
    rect(0, 1 - eighths / 8, 1, eighths / 8);
  } else if (cp >= 0x2589 && cp <= 0x258f) {
    // ▉ to ▏: the left eighths, from seven down to one
    rect(0, 0, (0x2590 - cp) / 8, 1);
  } else if (cp === 0x2590)
    rect(0.5, 0, 0.5, 1); // ▐
  else if (cp >= 0x2591 && cp <= 0x2593) {
    // ░ ▒ ▓: shades of a quarter, a half and three quarters
    const alpha = pen.globalAlpha;
    pen.globalAlpha = alpha * ((cp - 0x2590) / 4);
    rect(0, 0, 1, 1);
    pen.globalAlpha = alpha;
  } else if (cp === 0x2594)
    rect(0, 0, 1, 1 / 8); // ▔
  else if (cp === 0x2595)
    rect(7 / 8, 0, 1 / 8, 1); // ▕
  else {
    // ▖ to ▟: quadrants, as upper left, upper right, lower left, lower right
    const QUADRANTS: Record<number, string> = {
      0x2596: "0010",
      0x2597: "0001",
      0x2598: "1000",
      0x2599: "1011",
      0x259a: "1001",
      0x259b: "1110",
      0x259c: "1101",
      0x259d: "0100",
      0x259e: "0110",
      0x259f: "0111",
    };
    const quads = QUADRANTS[cp];
    if (!quads) return false;
    [...quads].forEach((on, i) => {
      if (on === "1") rect((i % 2) / 2, Math.floor(i / 2) / 2, 0.5, 0.5);
    });
  }
  return true;
}

/** Braille: eight dots in two columns of four, as its bits say. */
function drawBraille(pen: Pen, bits: number, { x, y, w, h }: Box) {
  // the bit of each dot, by column and row
  const DOTS = [
    [0, 1, 2, 6],
    [3, 4, 5, 7],
  ];
  const r = Math.max(1, Math.min(w / 4, h / 8) * 0.8);
  for (let col = 0; col < 2; col++) {
    for (let row = 0; row < 4; row++) {
      if (!(bits & (1 << DOTS[col][row]))) continue;
      pen.beginPath();
      pen.arc(x + (w * (col * 2 + 1)) / 4, y + (h * (row * 2 + 1)) / 8, r, 0, Math.PI * 2);
      pen.fill();
    }
  }
}

/** The Powerline separators: solid and thin arrows and half circles. */
function drawPowerline(pen: Pen, cp: number, { x, y, w, h }: Box) {
  const pointsRight = cp === 0xe0b0 || cp === 0xe0b1 || cp === 0xe0b4 || cp === 0xe0b5;
  const solid = cp === 0xe0b0 || cp === 0xe0b2 || cp === 0xe0b4 || cp === 0xe0b6;
  const round = cp >= 0xe0b4;
  const [back, tip] = pointsRight ? [x, x + w] : [x + w, x];
  pen.beginPath();
  if (round) {
    const r = h / 2;
    pen.arc(back, y + r, r, -Math.PI / 2, Math.PI / 2, !pointsRight);
  } else {
    pen.moveTo(back, y);
    pen.lineTo(tip, y + h / 2);
    pen.lineTo(back, y + h);
  }
  if (solid) {
    pen.closePath();
    pen.fill();
  } else {
    pen.lineWidth = lightWidth({ x, y, w, h });
    pen.strokeStyle = pen.fillStyle;
    pen.stroke();
  }
}
