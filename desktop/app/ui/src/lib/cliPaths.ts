// How the CLI's settings row writes the paths of the installed CLI, on Unix
// and on Windows.

/** A function that puts ~ in place of the home folder, which `link`, the installed CLI's path, starts with. */
export const tilde = (link: string) => {
  const found = /^(.*?)([\\/])\.local[\\/]bin[\\/]pinrail(\.exe)?$/.exec(link);
  const home = found?.[1];
  const sep = found?.[2] ?? "/";
  return (p: string) => (home && p.startsWith(home + sep) ? "~" + p.slice(home.length) : p);
};

/** The folder the CLI at `p` is in. */
export const folder = (p: string) => p.replace(/[\\/]pinrail(\.exe)?$/, "");
