// Menu aim: whether the pointer is on its way to an open submenu. A move
// heads for it when it ends inside the triangle between where it started
// and the submenu's near edge; the rows of the menu it crosses on the way
// should not take the submenu's place.

export type Point = { x: number; y: number };
export type Box = { left: number; right: number; top: number; bottom: number };

/** Which side of the line from a to b the point p lies on. */
const side = (a: Point, b: Point, p: Point) => (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);

/** Whether the move from `from` to `to` heads for the submenu in `box`,
 *  on whichever side of the pointer it opens. */
export function headsFor(from: Point, to: Point, box: Box): boolean {
  let edge: number;
  if (from.x <= box.left) edge = box.left;
  else if (from.x >= box.right) edge = box.right;
  else return false;
  // over the edge, the pointer is over the submenu or past it
  if (edge === box.left ? to.x >= edge : to.x <= edge) return false;
  const top = { x: edge, y: box.top };
  const bottom = { x: edge, y: box.bottom };
  // strictly inside the triangle: on the same side of each of its edges
  const a = side(from, top, to);
  const b = side(top, bottom, to);
  const c = side(bottom, from, to);
  return (a > 0 && b > 0 && c > 0) || (a < 0 && b < 0 && c < 0);
}
