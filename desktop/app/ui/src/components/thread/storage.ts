// Choices about a thread kept on this machine, such as whether its details
// show, which last until the app restarts when there is no storage.

export function stored(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

export function store(key: string, value: string) {
  try {
    localStorage.setItem(key, value);
  } catch {
    // without storage, the choice lasts until the app restarts
  }
}
