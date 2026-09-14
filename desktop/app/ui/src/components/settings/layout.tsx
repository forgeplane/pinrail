// A settings page is a title and groups; a group is a caption over a card
// of rows; a row is a label, one line about it, and the control at the end.

import type { ReactNode } from "react";

export function SettingsPage({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="settings-page">
      <h2>{title}</h2>
      {children}
    </section>
  );
}

export function SettingsGroup({ caption, children }: { caption?: string; children: ReactNode }) {
  return (
    <div className="settings-group">
      {caption ? <h3>{caption}</h3> : null}
      <div className="settings-card">{children}</div>
    </div>
  );
}

export function SettingsRow({ label, description, children, note }: { label: string; description?: ReactNode; children?: ReactNode; note?: ReactNode }) {
  return (
    <div className="settings-row">
      <div className="settings-text">
        <div className="settings-label">{label}</div>
        {description ? <div className="settings-desc">{description}</div> : null}
        {note ? <div className="settings-note">{note}</div> : null}
      </div>
      {children ? <div className="settings-control">{children}</div> : null}
    </div>
  );
}
