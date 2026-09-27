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

export function SettingsGroup({ caption, action, children }: { caption?: string; action?: ReactNode; children: ReactNode }) {
  return (
    <div className="settings-group">
      {caption || action ? (
        <div className="settings-group-head">
          {caption ? <h3>{caption}</h3> : null}
          {action}
        </div>
      ) : null}
      <div className="settings-card">{children}</div>
    </div>
  );
}

export function SettingsRow({ label, description, children, note, icon, onClick }: { label: string; description?: ReactNode; children?: ReactNode; note?: ReactNode; icon?: ReactNode; onClick?: () => void }) {
  return (
    <div className={`settings-row ${onClick ? "is-clickable" : ""}`}>
      {icon ? (
        <span className="settings-row-icon" onClick={onClick}>
          {icon}
        </span>
      ) : null}
      <div className="settings-text" onClick={onClick}>
        <div className="settings-label">{label}</div>
        {description ? <div className="settings-desc">{description}</div> : null}
        {note ? (
          // a control in the note does what it says, and only that: its
          // click does not also fold the row
          <div
            className="settings-note"
            onClick={(event) => {
              if ((event.target as HTMLElement).closest("button, a, input, select, textarea")) event.stopPropagation();
            }}
          >
            {note}
          </div>
        ) : null}
      </div>
      {children ? <div className="settings-control">{children}</div> : null}
    </div>
  );
}
