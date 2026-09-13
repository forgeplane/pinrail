import { Archive, CircleDot, SearchX } from "lucide-react";
import { useEffect, useState } from "react";
import { Link, useSearchParams } from "react-router";
import { api } from "../api/client";
import type { Review } from "../api/types";
import { StatusBadge } from "../components/Badges";
import { EmptyState } from "../components/EmptyState";
import { Select } from "../components/Select";
import { stamp } from "../lib/format";
import { useLive } from "../state/live";

const KEYS = ["status", "repo", "workflow", "ref", "plugin"] as const;

const settledAt = (r: Review) => r.decision?.decided_at ?? r.withdrawn_at ?? r.expires_at;

export function History() {
  const live = useLive();
  const [params, setParams] = useSearchParams();
  const [reviews, setReviews] = useState<Review[]>([]);
  const filters = Object.fromEntries(KEYS.map((k) => [k, params.get(k) ?? ""]));
  const filtered = KEYS.some((k) => filters[k]);

  useEffect(() => {
    api
      .listReviews({
        status: filters.status || "decided,withdrawn,expired",
        repo: filters.repo,
        workflow: filters.workflow,
        ref: filters.ref,
        plugin: filters.plugin,
        include_revised: "true",
      })
      .then(setReviews)
      .catch(() => setReviews([]));
  }, [filters.status, filters.repo, filters.workflow, filters.ref, filters.plugin, live.tick]);

  const setFilter = (key: string, value: string) => {
    const next = new URLSearchParams(params);
    if (value) next.set(key, value);
    else next.delete(key);
    setParams(next, { replace: true });
  };

  return (
    <div className="history">
      <header className="page-head">
        <h1>History</h1>
      </header>
      <div className="history-filters">
        <Select
          label="Status"
          icon={<CircleDot size={14} />}
          value={filters.status}
          onChange={(v) => setFilter("status", v)}
          options={[
            { value: "", label: "All outcomes" },
            { value: "decided", label: "Decided" },
            { value: "withdrawn", label: "Withdrawn" },
            { value: "expired", label: "Expired" },
          ]}
        />
        {(["repo", "workflow", "ref", "plugin"] as const).map((key) => (
          <input key={key} aria-label={key} placeholder={`${key[0].toUpperCase()}${key.slice(1)}…`} value={filters[key]} onChange={(e) => setFilter(key, e.target.value)} />
        ))}
        {filtered ? (
          <button type="button" className="chrome-button" onClick={() => setParams({}, { replace: true })}>
            Clear filters
          </button>
        ) : null}
      </div>
      {reviews.length === 0 ? (
        <EmptyState
          title={filtered ? "No matching decisions" : "Your decisions belong here"}
          icon={filtered ? <SearchX size={28} strokeWidth={1.5} /> : <Archive size={28} strokeWidth={1.5} />}
        >
          {filtered ? "Nothing matches these filters." : "Decided, withdrawn and expired reviews appear here, with the view they were decided in."}
        </EmptyState>
      ) : (
        <div className="history-table-wrap">
          <table className="history-table">
            <thead>
              <tr>
                <th>Review / requester</th>
                <th>Outcome</th>
                <th>Repository</th>
                <th>Recorded</th>
              </tr>
            </thead>
            <tbody>
              {reviews.map((r) => (
                <tr key={r.id}>
                  <td>
                    <Link to={`/reviews/${r.id}`} className="history-title">
                      {r.title}
                    </Link>
                    <small>
                      {r.requested_by}
                      {r.requested_by ? " · " : ""}
                      {r.plugin}
                    </small>
                  </td>
                  <td>
                    <StatusBadge status={r.status} />
                  </td>
                  <td>
                    {r.origin.repo}
                    {r.origin.ref ? <small className="mono">{r.origin.ref}</small> : null}
                  </td>
                  <td title={stamp(settledAt(r))}>
                    {stamp(settledAt(r))}
                    {r.decision ? <small>Decided by {r.decision.decided_by}</small> : null}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
