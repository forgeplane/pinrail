// The plugins the gallery lists, as scripts/plugin-gallery.mjs writes them
// from their manifests.
import data from "../../data/plugins.json";

export type Plugin = {
  name: string;
  title: string;
  version: string;
  description: string;
  use_when: string;
  /** comes with the app; otherwise a sample in forgeplane/pinrail-plugins */
  official: boolean;
  /** the setup installs it */
  recommended: boolean;
  takes_files: boolean;
  /** a sample whose view is built before it installs */
  build: boolean;
  /** its docs page, or its README */
  docs: string;
  image: { shot: string; dark: boolean } | { readme: string };
};

export const plugins = data as Plugin[];

export const samplesRepo = "https://github.com/forgeplane/pinrail-plugins";

/** What to give an agent so it installs the plugin: the plugin's page says how. */
export const agentLine = (p: Plugin) =>
  `Install the Pinrail plugin ${p.name}, as https://pinrail.dev/plugins/${p.name}/ describes.`;

/** The commands that install it, one a line. */
export function commands(p: Plugin): string[] {
  if (p.official) return [`pinrail plugins install ${p.name}`];
  return [
    `git clone ${samplesRepo}.git`,
    "cd pinrail-plugins",
    ...(p.build ? [`(cd plugins/${p.name} && npm ci && npm run build)`] : []),
    `pinrail plugins install ./plugins/${p.name}`,
  ];
}
