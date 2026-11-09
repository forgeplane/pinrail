// Dragging one kind of thing onto targets on a page: what is dragged
// carries a value under its own type, a target takes only that type, and
// the target under the pointer is lit until the pointer leaves it.

import { useState, type DragEvent } from "react";

export function useDragDrop(type: string) {
  // the key of the target under the pointer
  const [dropOn, setDropOn] = useState<string | null>(null);

  /** Starts a drag that carries `value`. */
  const start = (event: DragEvent, value: string) => {
    event.dataTransfer.setData(type, value);
    event.dataTransfer.effectAllowed = "move";
  };
  /** Over the target `key`: lit, and taking the drop when it accepts it. */
  const over = (event: DragEvent, key: string, accepts = true) => {
    if (!accepts || !event.dataTransfer.types.includes(type)) return;
    event.preventDefault();
    event.dataTransfer.dropEffect = "move";
    setDropOn(key);
  };
  /** Leaving a target, and not for one of its own children. */
  const leave = (event: DragEvent) => {
    if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setDropOn(null);
  };
  /** The value dropped, or null when it is not this type. */
  const take = (event: DragEvent): string | null => {
    event.preventDefault();
    setDropOn(null);
    return event.dataTransfer.getData(type) || null;
  };
  const end = () => setDropOn(null);

  return {
    dropOn,
    start,
    over,
    leave,
    take,
    end,
    /** The props of something dragged that carries `value`. */
    source: (value: string, onEnd?: () => void) => ({
      draggable: true,
      onDragStart: (event: DragEvent) => start(event, value),
      onDragEnd: () => {
        end();
        onEnd?.();
      },
    }),
    /** The props of a target, which hands what is dropped on it to `onDrop`. */
    target: (key: string, onDrop: (value: string, event: DragEvent) => void, accepts = true) => ({
      onDragOver: (event: DragEvent) => over(event, key, accepts),
      onDragLeave: leave,
      onDrop: (event: DragEvent) => {
        const value = take(event);
        if (value && accepts) onDrop(value, event);
      },
    }),
  };
}
