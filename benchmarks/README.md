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

Run:

```sh
scripts/benchmarks/run-agent-debug-benchmark.py --scenario-dir benchmarks/scenarios --output tmp/benchmark-report.json
```
