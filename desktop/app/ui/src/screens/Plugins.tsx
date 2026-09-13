import { Blocks, CircleCheck, ExternalLink, FolderPlus, RefreshCw, TriangleAlert, Wrench } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { ApiError, api, serverUrl } from "../api/client";
import type { Plugin } from "../api/types";
import { PluginBadge } from "../components/Badges";
import { EmptyState } from "../components/EmptyState";
import { Tooltip } from "../components/Tooltip";
import { useLive } from "../state/live";

export function Plugins() {
  const live = useLive();
  const [plugins, setPlugins] = useState<Plugin[]>([]);
  const [dirs, setDirs] = useState<string[]>([]);
  const [base, setBase] = useState("");
  const [message, setMessage] = useState<string | null>(null);
  const [newDir, setNewDir] = useState("");

  const load = useCallback(async () => {
    const { plugins, dirs } = await api.plugins();
    setPlugins(plugins);
    setDirs(dirs);
  }, []);

  useEffect(() => {
    load().catch(() => {});
    serverUrl().then(setBase);
  }, [load, live.tick]);

  const reload = async () => {
    try {
      const { count } = await api.reloadPlugins();
      setMessage(`Reloaded ${count} plugins`);
    } catch (e) {
      setMessage(e instanceof Error ? e.message : "Reload failed");
    }
    load().catch(() => {});
  };

  const add = async () => {
    if (!newDir.trim()) return;
    try {
      const { count } = await api.addPluginDir(newDir.trim());
      setMessage(`${count} plugins registered`);
      setNewDir("");
    } catch (e) {
      setMessage(e instanceof ApiError ? e.violations[0]?.message ?? e.message : "Could not add the directory");
    }
    load().catch(() => {});
  };

  return (
    <div className="plugins">
      <header className="page-head">
        <h1>Plugins</h1>
        <Tooltip label="Read the plugin directories again">
          <button type="button" className="chrome-button" onClick={reload}>
            <RefreshCw size={14} /> Reload
          </button>
        </Tooltip>
      </header>
      {message ? <p className="notice">{message}</p> : null}
      {plugins.length === 0 ? (
        <EmptyState title="A view for every decision" icon={<Blocks size={28} strokeWidth={1.5} />}>No plugins are registered. Point wicket at a directory of plugins to give your agents a view to ask through.</EmptyState>
      ) : (
        <div className="plugin-list">
          {plugins.map((p) => (
            <article key={p.name} className="plugin-row">
              <div>
                <h2>{p.title || p.name}</h2>
                <div className="mt">
                  <PluginBadge name={p.name} version={p.version} />
                </div>
              </div>
              <div className="plugin-path">
                <div>{p.path}</div>
                {p.usable ? (
                  <div className="mt">
                    entry {p.entry} ·{" "}
                    <a href={`${base}/plugins/${p.name}/${p.version}/${p.entry}`} target="_blank" rel="noreferrer" className="with-icon">
                      bundle <ExternalLink size={12} />
                    </a>
                  </div>
                ) : null}
              </div>
              <span className={`with-icon ${p.error ? "danger" : "ok"}`}>
                {p.error ? <TriangleAlert size={14} /> : p.dev ? <Wrench size={14} /> : <CircleCheck size={14} />}
                {p.error ? "broken" : p.dev ? "development" : "ready"}
              </span>
              {p.error ? <p className="plugin-error">{p.error}</p> : null}
            </article>
          ))}
        </div>
      )}
      <section className="plugin-dirs">
        <h2>Plugin directories</h2>
        <ul className="mono">
          {dirs.map((d) => (
            <li key={d}>{d}</li>
          ))}
        </ul>
        <div className="add-dir">
          <input aria-label="Directory to add" placeholder="/path/to/plugins" value={newDir} onChange={(e) => setNewDir(e.target.value)} onKeyDown={(e) => e.key === "Enter" && add()} />
          <button type="button" className="chrome-button" onClick={add}>
            <FolderPlus size={14} /> Add directory
          </button>
        </div>
      </section>
    </div>
  );
}
