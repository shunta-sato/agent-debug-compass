# Sensor Gateway Bugs Demo

This demo uses a small buggy sensor gateway app to show why `adc-targetd` is
useful during RCA-oriented debugging.

Run the fast local version:

```bash
scripts/demo/run-sensor-gateway-demo.sh --quick
```

Open:

```text
demo-results/sensor-gateway/agent_context.md
```

What to look for:

- Retry-storm warnings are visible in the evidence/window path.
- Memory-leak behavior is visible through before/after comparison.
- Raw app events, command stdout/stderr, and kmsg fixture are referenced instead
  of pasted into the Agent context.
- MCP exposes bounded RCA tools and not an arbitrary shell tool.

The demo is intentionally non-root by default. Raspberry Pi 5 specific evidence
can be layered on with the target smoke scripts after the basic story is proven.
