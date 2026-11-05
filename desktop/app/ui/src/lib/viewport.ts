// Where an element is in the window, for a popup drawn at the end of the
// body. The interface size zooms #root with CSS zoom, and engines disagree
// on getBoundingClientRect inside it: newer ones answer in the window's
// pixels, older WebKit (Safari 18's) before the zoom. A popup outside #root
// is placed in the window's pixels, so a rectangle given before the zoom is
// scaled up by it.

/** How much to scale a rectangle the engine reported inside #root: the
 *  zoom, over how much the engine already applied to what it reports. */
export function zoomCorrection(zoom: number, reportedWidth: number, layoutWidth: number): number {
  if (!zoom || zoom === 1 || !layoutWidth || !reportedWidth) return 1;
  const applied = reportedWidth / layoutWidth;
  const factor = zoom / applied;
  return Math.abs(factor - 1) < 0.001 ? 1 : factor;
}

/** A rectangle scaled from the window's top left corner. */
export function scaleRect(rect: { left: number; top: number; width: number; height: number }, factor: number) {
  const left = rect.left * factor;
  const top = rect.top * factor;
  const width = rect.width * factor;
  const height = rect.height * factor;
  return { left, top, width, height, right: left + width, bottom: top + height };
}

/** The element's rectangle in the window's pixels, whatever the engine. */
export function viewportRect(element: Element) {
  const rect = element.getBoundingClientRect();
  const root = document.getElementById("root");
  if (!root || !root.contains(element)) return scaleRect(rect, 1);
  const zoom = parseFloat(getComputedStyle(root).zoom) || 1;
  return scaleRect(rect, zoomCorrection(zoom, root.getBoundingClientRect().width, root.offsetWidth));
}
