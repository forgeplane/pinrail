// Values kept on this machine, in localStorage, or in sessionStorage for
// what lasts only while the window is open. Without storage, such as in a
// private window, a value lasts until the app restarts.

import { useState } from "react";

const area = (session?: boolean) => (session ? sessionStorage : localStorage);

/** The value stored under `key`, or null. */
export function readStored(key: string, { session = false }: { session?: boolean } = {}): string | null {
  try {
    return area(session).getItem(key);
  } catch {
    return null;
  }
}

/** Stores `value` under `key`, or removes the key for null. */
export function writeStored(key: string, value: string | null, { session = false }: { session?: boolean } = {}) {
  try {
    if (value === null) area(session).removeItem(key);
    else area(session).setItem(key, value);
  } catch {
    // without storage, the value lasts until the app restarts
  }
}

/** A string kept in storage under `key`, read once as the component mounts. */
export function useStoredState(key: string, initial: string, options: { session?: boolean } = {}) {
  const [value, setValue] = useState(() => readStored(key, options) ?? initial);
  const keep = (next: string) => {
    setValue(next);
    writeStored(key, next, options);
  };
  return [value, keep] as const;
}
