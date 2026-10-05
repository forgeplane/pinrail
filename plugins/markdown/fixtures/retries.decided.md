# Retry policy for webhook deliveries

markdown · acme/api · review · 42
Decided by Maya at 2026-10-03 12:00

**Changes requested**: docs/retries.md

## Comments

1. **diagram** in *Retry policy for webhook deliveries › Design*, lines 16–23: “flowchart LR”
   Show where the Retry-After header comes in.
2. **Question** on a **text** in *Retry policy for webhook deliveries › Design › Backoff*, lines 34–34: “Jitter of up to 20 % spreads retries from many workers.”
   Why 20 %?
