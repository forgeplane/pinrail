import { useEffect, useState } from "react";
import { getInfo, type Info } from "./api/client";

export function App() {
  const [info, setInfo] = useState<Info | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    getInfo().then(setInfo, (e: Error) => setError(e.message));
  }, []);

  return (
    <main className="shell">
      <header className="topbar">
        <span className="wordmark">wicket</span>
        <span className="count" aria-label="pending reviews">0 pending</span>
      </header>
      <section className="empty">
        <h1>No reviews waiting</h1>
        <p>Submit one from a terminal to see it here:</p>
        <pre>wicket submit code_review --data review.json --wait</pre>
        {info && (
          <p className="meta">
            server {info.version} · port {info.port} · {info.data_dir}
          </p>
        )}
        {error && <p className="meta error">The server did not answer: {error}</p>}
      </section>
    </main>
  );
}
