# Agent Debugging Benchmark

This benchmark measures whether Agent Debug Compass outputs help an AI Agent advance a falsifiable investigation safely.

The MVP benchmark uses checked-in scenarios and static investigation-state expectations. It is not a claim of platform leadership. It is a contract harness for measuring:

- hallucinated cause claims,
- unsafe probe suggestions,
- ignored `data_quality`,
- hypothesis ranking,
- unnecessary raw ref access,
- evidence-supported statements,
- time to first useful probe.

The `camera_inference_degradation_flight_recorder` scenario also compares:

- direct shell after Ty,
- ADC on-demand collection after Ty,
- ADC Flight Recorder with a retained Tx pre-window.

The Flight Recorder benchmark does not score root-cause accuracy. It scores
whether pre-window evidence, loss reports, coverage, bounded refs, and resource
discipline give the Agent a better falsifiable investigation starting point
without exceeding recorder overhead or resource budgets. In particular, the
Flight Recorder scenario asserts that the continuous memory ring does not create
a continuous disk-write path.

Run:

```sh
scripts/benchmarks/run-agent-debug-benchmark.py --scenario-dir benchmarks/scenarios --output tmp/benchmark-report.json
```
