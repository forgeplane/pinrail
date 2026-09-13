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
  serverDir: string;
  /** set when WICKET_E2E_SERVER=desktop: the headless desktop binary runs the server */
  desktopBin?: string;
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
    WICKET_DATA_DIR: state.dataDir,
    WICKET_CONFIG_DIR: state.configDir,
    WICKET_PORT: String(state.port),
    WICKET_URL: undefined,
    WICKET_SERVER_CMD: undefined,
    WICKET_SERVER_DIR: undefined,
  };
  if (state.desktopBin) {
    env.WICKET_SERVER_CMD = `exec "${state.desktopBin}" --headless --port ${state.port} --data-dir "${state.dataDir}"`;
  } else {
    env.WICKET_SERVER_DIR = state.serverDir;
  }
  return env;
}

export function serverPid(state: State = loadState()): number | null {
  try {
    return JSON.parse(fs.readFileSync(path.join(state.dataDir, "server.json"), "utf8")).pid ?? null;
  } catch {
    return null;
  }
}
