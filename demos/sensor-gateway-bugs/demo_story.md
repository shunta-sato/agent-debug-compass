# Demo Story

## Message

`adc-targetd` reduces information debt during bug investigation. It does not
declare a root cause. It collects evidence, aligns it, summarizes it, and gives
an Agent safe handles for deeper inspection.

## Flow

1. Run a baseline sensor gateway scenario.
2. Run a retry-storm scenario where failed sends are retried without backoff.
3. Run a memory-leak scenario where error buffers are retained.
4. Read `agent_context.md`.
5. Follow the bounded path from evidence index to window, event search,
   compare output, and raw refs.

## Contrast

Without `adc-targetd`, a developer has to manually collect app logs, stderr,
system snapshots, timestamps, and before/after notes.

With `adc-targetd`, the first Agent prompt can start from a small context that
already points to the relevant window and artifacts.
