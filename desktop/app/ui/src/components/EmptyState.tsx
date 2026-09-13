import type { ReactNode } from "react";

export function EmptyState({ title, children, actions }: { title: string; children?: ReactNode; actions?: ReactNode }) {
  return (
    <section className="empty-state" role="status">
      <h2>{title}</h2>
      {children ? <p>{children}</p> : null}
      {actions ? <div className="empty-actions">{actions}</div> : null}
    </section>
  );
}
