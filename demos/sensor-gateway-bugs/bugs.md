# Bug Notes

This file is intentionally a spoiler for presenters.

## Retry Storm

The retry-storm scenario simulates a sender that retries immediately after a
synthetic delivery failure. It emits warning evidence and local UDP traffic.

Expected investigation clue:

- warning line contains `demo retry storm`
- app events contain `retry_attempt`
- network and kmsg evidence can be searched from the same run context

## Memory Leak

The memory-leak scenario simulates retaining an error buffer instead of
releasing it. The retained size is bounded by the `--retained-kb` argument.

Expected investigation clue:

- command summary includes `retained_bytes`
- before/after comparison reports memory deltas
- raw events are referenced, not dumped into the Agent context
