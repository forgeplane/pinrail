// The setup's plugins view: a card for each plugin the app carries. The
// recommended ones are included, and the person picks any others to add.

import { Check, ChevronLeft, Plus } from "lucide-react";
import { useState } from "react";
import type { CatalogEntry } from "../../api/types";
import { PluginIcon } from "../PluginIcon";

/** A plugin the setup installs without being asked. */
export const included = (e: CatalogEntry) => e.recommended && !e.installed && !e.needs;

/** A plugin the person can add or leave. */
const offered = (e: CatalogEntry) => !e.recommended && !e.installed && !e.needs;

export function SetupPlugins({
  catalog,
  picked,
  onDone,
  onBack,
}: {
  /** the plugins the app carries, in the order to show them */
  catalog: CatalogEntry[] | null;
  /** the ids of the plugins the person added */
  picked: Set<string>;
  onDone: (picked: Set<string>) => void;
  onBack: () => void;
}) {
  // what is picked here counts once the button is pressed; Back leaves it
  const [draft, setDraft] = useState(picked);
  const toggle = (id: string) =>
    setDraft((d) => {
      const next = new Set(d);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  const entries = catalog ?? [];
  const count = entries.filter((e) => offered(e) && draft.has(e.id)).length;

  return (
    <>
      <button type="button" className="setup-corner is-back" onClick={onBack}>
        <ChevronLeft size={14} strokeWidth={2.2} /> Back
      </button>
      <div className="setup-body is-plugins">
        <h1 className="setup-title is-view">Add more plugins</h1>
        <p className="setup-sub is-wide">Each plugin is one kind of review an agent can ask you for.</p>
        {/* the cards scroll under the title, and fade out at the bottom edge */}
        <div className="setup-plugins-scroll">
          <div className="setup-plugins">
            {entries.map((e) => {
              const fixed = !offered(e);
              const on = !!e.installed || included(e) || (!fixed && draft.has(e.id));
              const text = e.description ?? e.use_when;
              return (
                <button
                  key={e.id}
                  type="button"
                  className="setup-plugin"
                  aria-pressed={on}
                  aria-disabled={fixed || undefined}
                  data-testid={`setup-plugin-${e.name}`}
                  onClick={fixed ? undefined : () => toggle(e.id)}
                >
                  <span className="setup-plugin-head">
                    <span className="setup-plugin-icon">
                      <PluginIcon icon={e.icon} size={17} />
                    </span>
                    {e.needs ? (
                      <span className="setup-plugin-state is-off">Needs Pinrail {e.needs}</span>
                    ) : e.installed ? (
                      <span className="setup-plugin-state">Installed</span>
                    ) : e.recommended ? (
                      <span className="setup-plugin-state">Included</span>
                    ) : null}
                    {e.needs ? null : (
                      <span className="setup-plugin-mark">
                        {on ? <Check size={11} strokeWidth={3.2} /> : <Plus size={11} strokeWidth={2.6} />}
                      </span>
                    )}
                  </span>
                  <b>{e.title}</b>
                  {text ? <span className="setup-plugin-text">{text}</span> : null}
                </button>
              );
            })}
          </div>
        </div>
      </div>
      <div className="setup-foot">
        <button type="button" className="chrome-button button-primary setup-button" onClick={() => onDone(draft)}>
          {count === 0 ? "Done" : count === 1 ? "Add 1 plugin" : `Add ${count} plugins`}
        </button>
      </div>
    </>
  );
}
