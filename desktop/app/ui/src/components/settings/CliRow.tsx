// The Install the CLI row: puts the pinrail CLI the app bundles into
// ~/.local/bin, as a link on macOS and a copy from an AppImage, or says a Linux
// package installed it already. It also says what a new terminal will actually
// run, since that is what an agent's shell gets.

import { useCallback, useEffect, useState, type ReactNode } from "react";
import { inTauri } from "../../api/client";
import { SettingsRow } from "./layout";
import { folder, tilde } from "../../lib/cliPaths";

export type CliStatus = {
  /** link on macOS, package for a .deb or .rpm, copy from an AppImage */
  mode: "link" | "package" | "copy";
  bundled: string | null;
  link: string;
  installed: boolean;
  /** a copy from another version is there */
  outdated: boolean;
  occupied_by: string | null;
  runs: string | null;
  dir_on_path: boolean | null;
  /** something that went wrong after installing, such as the PATH not updating */
  warning: string | null;
  /** the file the person's shell reads at start, where Pinrail can add the
   *  PATH line; null for a shell it does not know, and on Windows */
  profile: string | null;
  /** that line, in the shell's syntax */
  profile_line: string | null;
  /** the PATH is the user's value in the registry, as on Windows */
  user_path?: boolean;
};

const invoke = <T,>(command: string) => import("@tauri-apps/api/core").then(({ invoke }) => invoke<T>(command));

/** The CLI's status while `open`, looked at again when the window comes back
 * (from a terminal where the PATH was changed), and the install. */
export function useCli(open: boolean) {
  const [status, setStatus] = useState<CliStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [checking, setChecking] = useState(false);

  const check = useCallback(() => {
    setChecking(true);
    return invoke<CliStatus>("cli_status")
      .then((s) => setStatus(s))
      .catch((e) => setError(String(e)))
      .finally(() => setChecking(false));
  }, []);

  useEffect(() => {
    if (!open || !inTauri()) return;
    // eslint-disable-next-line react-hooks/set-state-in-effect -- asks the system whether the command is installed
    check();
    window.addEventListener("focus", check);
    return () => window.removeEventListener("focus", check);
  }, [open, check]);

  const install = useCallback(async () => {
    setBusy(true);
    setError(null);
    try {
      setStatus(await invoke<CliStatus>("install_cli"));
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }, []);

  /** Adds the PATH line to the shell's profile file, and says whether it did. */
  const addToPath = useCallback(async () => {
    setBusy(true);
    setError(null);
    try {
      setStatus(await invoke<CliStatus>("add_cli_to_path"));
      return true;
    } catch (e) {
      setError(String(e));
      return false;
    } finally {
      setBusy(false);
    }
  }, []);

  return { status, busy, error, install, check, checking, addToPath };
}

export function CliRow({ open }: { open: boolean }) {
  const { status, busy, error, install: installCli, addToPath } = useCli(open);
  // the profile file the PATH line was added to from here
  const [added, setAdded] = useState<string | null>(null);

  if (!inTauri())
    return <SettingsRow label="Install the CLI" description="Puts pinrail into ~/.local/bin; the app does this" />;
  if (!status) return <SettingsRow label="Install the CLI" description={error ?? "Checking…"} />;

  const short = tilde(status.link);
  const link = short(status.link);

  if (!status.bundled) {
    return (
      <SettingsRow
        label="Install the CLI"
        description="The packaged app carries the CLI and installs it from here; this development build does not"
      />
    );
  }

  const verb = status.mode === "copy" ? "Copies" : "Links";
  const description = status.installed ? (
    <span>
      {status.mode === "package" ? "Installed with the package at " : "Installed at "}
      <span className="mono">{link}</span>
    </span>
  ) : (
    <span>
      {verb} <span className="mono">{status.link.endsWith(".exe") ? "pinrail.exe" : "pinrail"}</span> into{" "}
      <span className="mono">{folder(link)}</span>, so agents and scripts can run it
    </span>
  );

  let note: ReactNode = undefined;
  if (error) note = error;
  else if (status.warning) note = `Installed, but ${status.warning}`;
  else if (status.outdated) note = `${link} is a copy from another version of Pinrail; installing replaces it`;
  else if (!status.installed && status.occupied_by)
    note = `${link} is already ${status.occupied_by}; installing replaces ${status.mode === "copy" ? "a link or an older copy" : "a link"}, never another file`;
  else if (added)
    note = status.user_path ? (
      <span>Added {folder(link)} to your PATH. Open a new terminal to use it</span>
    ) : (
      <span>
        Added {folder(link)} to your PATH in <span className="mono">{added}</span>. Open a new terminal to use it
      </span>
    );
  else if (status.installed && status.dir_on_path === false && status.user_path)
    note = (
      <span>
        {folder(link)} is not on your PATH. Add it from here, or run <span className="mono">{status.profile_line}</span>{" "}
        in PowerShell
      </span>
    );
  else if (status.installed && status.dir_on_path === false)
    note = (
      <span>
        {folder(link)} is not on your PATH. Add{" "}
        <span className="mono">{status.profile_line ?? 'export PATH="$HOME/.local/bin:$PATH"'}</span> to{" "}
        {status.profile ? <span className="mono">{short(status.profile)}</span> : "your shell profile"}
      </span>
    );
  else if (status.installed && status.runs && status.runs !== status.link)
    note = (
      <span>
        A new terminal runs <span className="mono">{short(status.runs)}</span> first, which is not this one
      </span>
    );
  // the line can be added from here for a shell Pinrail knows, and on
  // Windows to the user's PATH in the registry
  const profile = !added && status.installed && status.dir_on_path === false && status.profile;
  const target = profile ? short(profile) : status.user_path ? "your PATH" : null;
  const add =
    !added && status.installed && status.dir_on_path === false && target ? (
      <button
        type="button"
        className="chrome-button"
        onClick={async () => {
          if (await addToPath()) setAdded(target);
        }}
        disabled={busy}
      >
        Add it to {target}
      </button>
    ) : null;

  if (status.mode === "package")
    return (
      <SettingsRow label="Install the CLI" description={description} note={note}>
        {add}
      </SettingsRow>
    );

  return (
    <SettingsRow label="Install the CLI" description={description} note={note}>
      <span className="settings-row-buttons">
        {add}
        <button type="button" className="chrome-button" onClick={installCli} disabled={busy}>
          {busy ? "Installing…" : status.outdated ? "Update" : status.installed ? "Reinstall" : "Install"}
        </button>
      </span>
    </SettingsRow>
  );
}
