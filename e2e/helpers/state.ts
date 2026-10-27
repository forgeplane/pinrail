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
/** The variables by which the CLI recognises an agent and its session. The
 * agent that runs the tests is not the one the tests are about, so they
 * are cleared; a test that needs one sets it. */
const AGENT_VARS = [
  "AI_AGENT",
  "AGENT",
  "CLAUDECODE",
  "CLAUDE_CODE_ENTRYPOINT",
  "CLAUDE_CODE_SESSION_ID",
  "CODEX_THREAD_ID",
  "CODEX_SANDBOX",
  "CODEX_CI",
  "CODEX_SESSION_ID",
  "CURSOR_AGENT",
  "ANTIGRAVITY_AGENT",
  "ANTIGRAVITY_CONVERSATION_ID",
  "OPENCODE",
  "OPENCODE_PID",
  "OPENCODE_CLIENT",
  "OPENCODE_SESSION_ID",
  "GROK_AGENT",
  "GROK_SESSION_ID",
];

export function cliEnv(state: State = loadState()): NodeJS.ProcessEnv {
  const inherited = { ...process.env };
  for (const name of AGENT_VARS) delete inherited[name];
  const env: NodeJS.ProcessEnv = {
    ...inherited,
    PINRAIL_DATA_DIR: state.dataDir,
    PINRAIL_CONFIG_DIR: state.configDir,
    PINRAIL_PORT: String(state.port),
    PINRAIL_URL: undefined,
    // JSON, as a script reads it; a test of the markdown passes `--markdown`
    PINRAIL_JSON: "1",
    // the SDK views load from /sdk/v1, as the shell's build writes it,
    // with its markdown parser and icons
    PINRAIL_SDK_DIR: path.join(root, "desktop", "app", "sdk", "v1"),
    // run by sh, or by cmd on Windows, which has no exec
    PINRAIL_SERVER_CMD: `${process.platform === "win32" ? "" : "exec "}"${state.desktopBin}" --headless --port ${state.port} --data-dir "${state.dataDir}"`,
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
