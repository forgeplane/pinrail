// The Install the CLI row: puts the wicket CLI the app bundles into
// ~/.local/bin, as a link on macOS and a copy from an AppImage, or says a Linux
// package installed it already. It also says what a new terminal will actually
// run, since that is what an agent's shell gets.

import { useCallback, useEffect, useState, type ReactNode } from "react";
import { inTauri } from "../../api/client";
import { SettingsRow } from "./layout";

type CliStatus = {
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
};

const invoke = <T,>(command: string) => import("@tauri-apps/api/core").then(({ invoke }) => invoke<T>(command));

/** ~ in place of the home folder, which the link's path starts with. */
const tilde = (link: string) => {
  const home = /^(.*)\/\.local\/bin\/wicket$/.exec(link)?.[1];
  return (p: string) => (home && p.startsWith(home + "/") ? "~" + p.slice(home.length) : p);
};

const folder = (p: string) => p.replace(/\/wicket$/, "");

export function CliRow({ open }: { open: boolean }) {
  const [status, setStatus] = useState<CliStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const check = useCallback(() => {
    invoke<CliStatus>("cli_status")
      .then((s) => setStatus(s))
      .catch((e) => setError(String(e)));
  }, []);

  useEffect(() => {
    if (!open || !inTauri()) return;
    check();
    // back from a terminal where the PATH was changed: look again
    window.addEventListener("focus", check);
    return () => window.removeEventListener("focus", check);
  }, [open, check]);

  const installCli = async () => {
    setBusy(true);
    setError(null);
    try {
      setStatus(await invoke<CliStatus>("install_cli"));
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  if (!inTauri()) return <SettingsRow label="Install the CLI" description="Puts wicket into ~/.local/bin; the app does this" />;
  if (!status) return <SettingsRow label="Install the CLI" description={error ?? "Checking…"} />;

  const short = tilde(status.link);
  const link = short(status.link);

  if (!status.bundled) {
    return <SettingsRow label="Install the CLI" description="The packaged app carries the CLI and installs it from here; this development build does not" />;
  }

  const verb = status.mode === "copy" ? "Copies" : "Links";
  const description = status.installed ? (
    <span>
      {status.mode === "package" ? "Installed with the package at " : "Installed at "}
      <span className="mono">{link}</span>
    </span>
  ) : (
    <span>
      {verb} <span className="mono">wicket</span> into <span className="mono">{folder(link)}</span>, so agents and scripts can run it
    </span>
  );

  let note: ReactNode = undefined;
  if (error) note = error;
  else if (status.outdated) note = `${link} is a copy from another version of Wicket; installing replaces it`;
  else if (!status.installed && status.occupied_by)
    note = `${link} is already ${status.occupied_by}; installing replaces ${status.mode === "copy" ? "a link or an older copy" : "a link"}, never another file`;
  else if (status.installed && status.dir_on_path === false)
    note = (
      <span>
        {folder(link)} is not on your PATH. Add <span className="mono">export PATH="$HOME/.local/bin:$PATH"</span> to your shell profile
      </span>
    );
  else if (status.installed && status.runs && status.runs !== status.link)
    note = (
      <span>
        A new terminal runs <span className="mono">{short(status.runs)}</span> first, which is not this one
      </span>
    );
  else if (status.installed && status.mode === "copy") note = "A copy does not follow the AppImage: after updating Wicket, install again";

  if (status.mode === "package") return <SettingsRow label="Install the CLI" description={description} note={note} />;

  return (
    <SettingsRow label="Install the CLI" description={description} note={note}>
      <button type="button" className="chrome-button" onClick={installCli} disabled={busy} data-install-cli>
        {busy ? "Installing…" : status.outdated ? "Update" : status.installed ? "Reinstall" : "Install"}
      </button>
    </SettingsRow>
  );
}
