import fs from "node:fs";
import path from "node:path";

export const root = path.resolve(__dirname, "..", "..");
export const stateDir = path.resolve(__dirname, "..", ".state");
const stateFile = path.join(stateDir, "current.json");

export type State = {
  url: string;
  port: number;
  cli: string;
  dataDir: string;
  configDir: string;
  /** the desktop app's binary, run headless as the server */
  desktopBin: string;
};

export function saveState(state: State) {
  fs.mkdirSync(stateDir, { recursive: true });
  fs.writeFileSync(stateFile, JSON.stringify(state, null, 2));
}

export function loadState(): State {
  return JSON.parse(fs.readFileSync(stateFile, "utf8"));
}

/** The env the CLI needs to find (and start) this run's server. */
export function cliEnv(state: State = loadState()): NodeJS.ProcessEnv {
  const env: NodeJS.ProcessEnv = {
    ...process.env,
    PINRAIL_DATA_DIR: state.dataDir,
    PINRAIL_CONFIG_DIR: state.configDir,
    PINRAIL_PORT: String(state.port),
    PINRAIL_URL: undefined,
    // the SDK views load from /sdk/v1, as the shell's build writes it,
    // with its markdown parser and icons
    PINRAIL_SDK_DIR: path.join(root, "desktop", "app", "sdk", "v1"),
    PINRAIL_SERVER_CMD: `exec "${state.desktopBin}" --headless --port ${state.port} --data-dir "${state.dataDir}"`,
  };
  return env;
}

export function serverPid(state: State = loadState()): number | null {
  try {
    return JSON.parse(fs.readFileSync(path.join(state.dataDir, "server.json"), "utf8")).pid ?? null;
  } catch {
    return null;
  }
}
