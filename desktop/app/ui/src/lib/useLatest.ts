// A ref that holds the latest value a component rendered with, for a
// callback or a listener that runs later and must see it. It is set after
// the render is committed, so a render React throws away leaves it as it was.

import { useLayoutEffect, useRef } from "react";

export function useLatest<T>(value: T) {
  const ref = useRef(value);
  useLayoutEffect(() => {
    ref.current = value;
  });
  return ref;
}
