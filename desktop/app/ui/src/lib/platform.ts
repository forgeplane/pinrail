/** Whether the window runs on Windows, where the PATH is in the registry
 *  and a folder's separator is \\. */
export const onWindows = /Win/i.test(typeof navigator === "undefined" ? "" : navigator.platform);
