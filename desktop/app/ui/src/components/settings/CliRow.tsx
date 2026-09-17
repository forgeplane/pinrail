// The Install the CLI row: links the wicket CLI the app bundles into
// ~/.local/bin, and says what a new terminal will actually run, since that
// is what an agent's shell gets.

import { useCallback, useEffect, useState, type ReactNode } from "react";
import { inTauri } from "../../api/client";
import { SettingsRow } from "./layout";

type CliStatus = {
  bundled: string | null;
  link: string;
  installed: boolean;
  occupied_by: string | null;
  runs: string | null;
  dir_on_path: boolean | null;
};

const invoke = <T,>(command: string) => import("@tauri-apps/api/core").then(({ invoke }) => invoke<T>(command));

/** ~ in place of the home folder, which the link's path starts with. */
const tilde = (link: string) => {
  const home = link.replace(/\/\.local\/bin\/wicket$/, "");
  return (p: string) => (home && p.startsWith(home + "/") ? "~" + p.slice(home.length) : p);
};

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

  if (!inTauri()) return <SettingsRow label="Install the CLI" description="Links wicket into ~/.local/bin; the app does this" />;
  if (!status) return <SettingsRow label="Install the CLI" description={error ?? "Checking…"} />;

  const short = tilde(status.link);
  const link = short(status.link);

  if (!status.bundled) {
    return <SettingsRow label="Install the CLI" description="The packaged app carries the CLI and links it from here; this development build does not" />;
  }

  const description = status.installed ? (
    <span>
      Installed at <span className="mono">{link}</span>
    </span>
  ) : (
    <span>
      Links <span className="mono">wicket</span> into <span className="mono">{link.replace(/\/wicket$/, "")}</span>, so agents and scripts can run it
    </span>
  );

  let note: ReactNode = undefined;
  if (error) note = error;
  else if (!status.installed && status.occupied_by) note = `${link} is already ${status.occupied_by}; installing replaces a link, never a file`;
  else if (status.installed && status.dir_on_path === false)
    note = (
      <span>
        {link.replace(/\/wicket$/, "")} is not on your PATH. Add <span className="mono">export PATH="$HOME/.local/bin:$PATH"</span> to your shell profile
      </span>
    );
  else if (status.installed && status.runs && status.runs !== status.link)
    note = (
      <span>
        A new terminal runs <span className="mono">{short(status.runs)}</span> first, which is not this one
      </span>
    );

  return (
    <SettingsRow label="Install the CLI" description={description} note={note}>
      <button type="button" className="chrome-button" onClick={installCli} disabled={busy} data-install-cli>
        {busy ? "Installing…" : status.installed ? "Reinstall" : "Install"}
      </button>
    </SettingsRow>
  );
}
