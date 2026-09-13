import type { ReactNode } from "react";

export function EmptyState({ title, icon, children, actions }: { title: string; icon?: ReactNode; children?: ReactNode; actions?: ReactNode }) {
  return (
    <section className="empty-state" role="status">
      {icon ? <span className="empty-icon">{icon}</span> : null}
      <h2>{title}</h2>
      {children ? <p>{children}</p> : null}
      {actions ? <div className="empty-actions">{actions}</div> : null}
    </section>
  );
}
