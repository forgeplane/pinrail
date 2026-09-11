# wicket

A wicket is the small gate in a larger door: a person waits at it, looks at
what is being carried through, and lets it pass or not.

wicket is a standalone approval-gate service for agent workflows. A workflow
that is about to do something outward-facing (post review comments, silence a
Sentry issue, open an issue, push a branch) creates a gate with a JSON payload
and blocks. The web app shows the gate in an inbox, renders it with the view
registered for its type, records the human's decision, and the blocked
workflow resumes with that decision. Every decision is kept, so past rounds
can be rendered again and rejection reasons become training data.

Stack: Elixir, Phoenix LiveView and plain-file storage for the server
(`server/`). The CLI agents call is a Rust binary (`cli/`) talking HTTP to the
running app.

Status: early development.
