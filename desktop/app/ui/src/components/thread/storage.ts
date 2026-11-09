// Choices about a thread kept on this machine, such as whether its details
// show, which last until the app restarts when there is no storage.

import { readStored, writeStored } from "../../lib/storage";

export const stored = (key: string) => readStored(key);

export const store = (key: string, value: string) => writeStored(key, value);
