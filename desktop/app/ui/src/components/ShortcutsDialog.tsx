// The keyboard-shortcuts dialog: the app's keys, and under them the keys
// of the plugin whose view is open. A search field at the top narrows
// both by what a key does or by the key itself; Esc clears it, then
// closes the dialog.

import { Search, X } from "lucide-react";
import { Fragment, useEffect, useMemo, useRef, useState } from "react";
import type { PluginShortcut } from "../api/types";
import { SHORTCUTS, isShadowed, shortcutGlyphs } from "../lib/shortcuts";
import { PluginIcon } from "./PluginIcon";
import { Tooltip } from "./Tooltip";

type OpenPlugin = { name: string; title: string; icon: string | null; shortcuts: PluginShortcut[] };

/** Every word of the query starts a word of the label, or is one of the keys. */
const matches = (query: string, label: string, keys: string[][]) => {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (!words.length) return true;
  const labelWords = label.toLowerCase().split(/[\s/]+/).filter(Boolean);
  const glyphs = keys.flat().map((k) => k.toLowerCase());
  return words.every((w) => labelWords.some((lw) => lw.startsWith(w)) || glyphs.includes(w));
};

export function ShortcutsDialog({ plugin, onClose }: { plugin?: OpenPlugin; onClose: () => void }) {
  const [query, setQuery] = useState("");
  const field = useRef<HTMLInputElement>(null);

  useEffect(() => {
    field.current?.focus();
  }, []);

  const app = useMemo(() => SHORTCUTS.filter((s) => matches(query, s.what, s.keys)), [query]);
  const own = useMemo(
    () => (plugin?.shortcuts ?? []).map((s) => ({ ...s, glyphs: shortcutGlyphs(s.keys), shadowed: isShadowed(s.keys) })).filter((s) => matches(query, s.does, [s.glyphs])),
    [plugin, query],
  );

  return (
    <div className="app-dialog-backdrop shortcuts-backdrop" onClick={onClose}>
      <div className="app-dialog shortcuts-dialog" role="dialog" aria-labelledby="keyboard-title" onClick={(e) => e.stopPropagation()}>
        <div className="dialog-head">
          <h2 id="keyboard-title">Keyboard shortcuts</h2>
          <Tooltip label="Close" keys={["Esc"]}>
            <button type="button" className="bar-button" onClick={onClose} aria-label="Close">
              <X size={16} />
            </button>
          </Tooltip>
        </div>
        <label className="palette-field shortcuts-search">
          <Search size={15} aria-hidden="true" />
          <input
            ref={field}
            type="search"
            placeholder="Search shortcuts…"
            aria-label="Search shortcuts"
            autoComplete="off"
            autoCorrect="off"
            autoCapitalize="off"
            spellCheck={false}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => {
              // Esc empties the field first; a second one closes the dialog
              if (e.key === "Escape" && query) {
                e.stopPropagation();
                setQuery("");
              }
            }}
          />
        </label>
        <div className="shortcuts-body">
          {app.length ? (
            <dl className="shortcut-list">
              {app.map((s) => (
                <Fragment key={s.what}>
                  <dt>{s.what}</dt>
                  <dd>
                    {s.keys.map((combo, i) => (
                      <span key={i} className="combo">
                        {combo.map((k, j) => (
                          <kbd key={j}>{k}</kbd>
                        ))}
                      </span>
                    ))}
                  </dd>
                </Fragment>
              ))}
            </dl>
          ) : null}
          {plugin && own.length ? (
            <>
              <h3 className="shortcut-plugin-title">
                <PluginIcon icon={plugin.icon} size={14} />
                In {plugin.title}
              </h3>
              <dl className="shortcut-list" data-plugin-shortcuts>
                {own.map((s, i) => (
                  <Fragment key={i}>
                    <dt className={s.shadowed ? "is-shadowed" : ""}>
                      {s.does}
                      {s.shadowed ? <span className="faint"> · the app uses this key</span> : null}
                    </dt>
                    <dd className={s.shadowed ? "is-shadowed" : ""}>
                      <span className="combo">
                        {s.glyphs.map((k, j) => (
                          <kbd key={j}>{k}</kbd>
                        ))}
                      </span>
                    </dd>
                  </Fragment>
                ))}
              </dl>
            </>
          ) : null}
          {!app.length && !own.length ? <p className="dim shortcuts-empty">No shortcut matches "{query}".</p> : null}
        </div>
      </div>
    </div>
  );
}
