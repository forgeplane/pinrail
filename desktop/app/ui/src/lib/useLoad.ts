// Data loaded from the core, again when the dependencies change or on
// reload: the last answer, or the reason the last load failed. An answer
// that arrives after the dependencies changed again is dropped.

import { useEffect, useState, type DependencyList } from "react";
import { errorMessage } from "../api/client";

export function useLoad<T>(load: () => Promise<T>, deps: DependencyList) {
  const [data, setData] = useState<T | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [reloads, setReloads] = useState(0);
  useEffect(() => {
    let cancelled = false;
    load()
      .then((answer) => {
        if (cancelled) return;
        setData(answer);
        setError(null);
      })
      .catch((e) => !cancelled && setError(errorMessage(e)));
    return () => {
      cancelled = true;
    };
  }, [...deps, reloads]); // eslint-disable-line react-hooks/exhaustive-deps -- the caller's dependencies
  return { data, error, reload: () => setReloads((n) => n + 1) };
}
