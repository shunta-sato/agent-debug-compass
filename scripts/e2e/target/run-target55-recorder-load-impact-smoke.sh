#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
HOST="target55"
BINARY_DIR="$ROOT_DIR/target/debug"
RESULT_ROOT="$ROOT_DIR/tmp/target55-recorder-load-impact-smoke"
REMOTE_ROOT="/tmp/adc-pr10-load-impact-${USER:-user}-$$"
DURATION_SECONDS=6
WORKLOAD_MEMORY_MB=64
KEEP_REMOTE=0

usage() {
  cat <<'USAGE'
Usage: run-target55-recorder-load-impact-smoke.sh [options]

Options:
  --host HOST              SSH host alias to use. Default: target55.
  --binary-dir DIR         Directory containing adc and adc-targetd. Default: target/debug.
  --result-root DIR        Local directory for smoke outputs. Default: tmp/target55-recorder-load-impact-smoke.
  --remote-root DIR        Remote temporary root. Default: /tmp/adc-pr10-load-impact-$USER-$$.
  --duration-seconds N     Per-phase workload duration. Default: 6.
  --workload-memory-mb N   Synthetic workload memory footprint. Default: 64.
  --keep-remote            Do not remove the remote temporary root after the smoke.
  --help                   Show this help.

This smoke compares the same CPU+memory workload on target55 with Flight
Recorder disabled, enabled with the normal policy, and enabled with simulated
battery_low policy. It is intentionally rootless and uses SSH only to deploy
fixed binaries/scripts and collect reports.
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --host)
      HOST="${2:?missing --host value}"
      shift 2
      ;;
    --binary-dir)
      BINARY_DIR="${2:?missing --binary-dir value}"
      shift 2
      ;;
    --result-root)
      RESULT_ROOT="${2:?missing --result-root value}"
      shift 2
      ;;
    --remote-root)
      REMOTE_ROOT="${2:?missing --remote-root value}"
      shift 2
      ;;
    --duration-seconds)
      DURATION_SECONDS="${2:?missing --duration-seconds value}"
      shift 2
      ;;
    --workload-memory-mb)
      WORKLOAD_MEMORY_MB="${2:?missing --workload-memory-mb value}"
      shift 2
      ;;
    --keep-remote)
      KEEP_REMOTE=1
      shift
      ;;
    --help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

ADC_BIN="$BINARY_DIR/adc"
TARGETD_BIN="$BINARY_DIR/adc-targetd"
if [[ ! -x "$ADC_BIN" ]]; then
  echo "missing executable adc at $ADC_BIN" >&2
  exit 1
fi
if [[ ! -x "$TARGETD_BIN" ]]; then
  echo "missing executable adc-targetd at $TARGETD_BIN" >&2
  exit 1
fi

rm -rf "$RESULT_ROOT"
mkdir -p "$RESULT_ROOT"

ssh -o BatchMode=yes -o ConnectTimeout=10 "$HOST" \
  'uname -a; id; command -v python3 || true; getconf CLK_TCK 2>/dev/null || true' \
  >"$RESULT_ROOT/target_identity.txt"

ssh "$HOST" "rm -rf '$REMOTE_ROOT' && mkdir -p '$REMOTE_ROOT/bin' '$REMOTE_ROOT/profiles' '$REMOTE_ROOT/results'"
scp "$ADC_BIN" "$TARGETD_BIN" "$HOST:$REMOTE_ROOT/bin/" >/dev/null
ssh "$HOST" "chmod 0755 '$REMOTE_ROOT/bin/adc' '$REMOTE_ROOT/bin/adc-targetd'"

ssh "$HOST" "cat > '$REMOTE_ROOT/profiles/recorder_load_impact.yaml'" <<'YAML'
profile: recorder_load_impact
sampling:
  interval_ms: 10
always_on:
  collectors: [memory, network]
budgets:
  max_daemon_cpu_percent: 3
  max_memory_mb: 128
  max_artifact_mb_per_run: 16
triggers: []
YAML

ssh "$HOST" "cat > '$REMOTE_ROOT/run_load_impact.py'" <<'PY'
#!/usr/bin/env python3
import argparse
import json
import os
import pathlib
import subprocess
import time


def read_proc_stats(pid, clock_ticks):
    try:
        stat_text = pathlib.Path(f"/proc/{pid}/stat").read_text(encoding="utf-8")
        after_comm = stat_text[stat_text.rfind(")") + 2 :].split()
        cpu_ticks = int(after_comm[11]) + int(after_comm[12])
        rss_kb = 0
        for line in pathlib.Path(f"/proc/{pid}/status").read_text(encoding="utf-8").splitlines():
            if line.startswith("VmRSS:"):
                rss_kb = int(line.split()[1])
                break
        return {"cpu_seconds": cpu_ticks / clock_ticks, "rss_kb": rss_kb}
    except FileNotFoundError:
        return None


def run_json(command, env, output_path):
    with output_path.open("w", encoding="utf-8") as out:
        subprocess.run(command, env=env, check=True, stdout=out, stderr=subprocess.PIPE, text=True)
    return json.loads(output_path.read_text(encoding="utf-8"))


def run_phase(name, mode, args, clock_ticks):
    phase_root = args.remote_root / name
    results = args.remote_root / "results"
    env = os.environ.copy()
    env["PATH"] = f"{args.remote_root / 'bin'}:{env.get('PATH', '')}"
    env["ADC_HOME"] = str(phase_root)
    env["ADC_PROFILE_DIR"] = str(args.remote_root / "profiles")

    targetd = None
    adc_start = None
    adc_latest = None
    adc_peak_rss_kb = 0

    if mode != "off":
        run_json(["adc", "arm", "--profile", "recorder_load_impact"], env, results / f"{name}_arm.json")
        recorder_env = env.copy()
        if mode == "battery_low":
            recorder_env["ADC_RECORDER_POWER_MODE"] = "battery_low"
        targetd = subprocess.Popen(
            ["adc-targetd", "--service-for-ms", str(int(args.duration_seconds * 1000) + 700)],
            env=recorder_env,
            stdout=(results / f"{name}_targetd_stdout.json").open("w", encoding="utf-8"),
            stderr=(results / f"{name}_targetd_stderr.txt").open("w", encoding="utf-8"),
            text=True,
        )
        time.sleep(0.08)
        adc_start = read_proc_stats(targetd.pid, clock_ticks)

    workload = bytearray(args.workload_memory_mb * 1024 * 1024)
    for idx in range(0, len(workload), 4096):
        workload[idx] = idx % 251

    start = time.monotonic()
    deadline = start + args.duration_seconds
    iterations = 0
    checksum = 0
    while time.monotonic() < deadline:
        for idx in range(0, len(workload), 4096):
            checksum = (checksum + workload[idx] + idx) & 0xFFFFFFFF
            workload[idx] = (workload[idx] + 1) % 251
        iterations += 1
        if targetd is not None:
            current = read_proc_stats(targetd.pid, clock_ticks)
            if current is not None:
                adc_latest = current
                adc_peak_rss_kb = max(adc_peak_rss_kb, current["rss_kb"])

    duration = time.monotonic() - start
    status = None
    adc_cpu_seconds = None
    adc_end = None

    if targetd is not None:
        targetd.wait(timeout=5)
        adc_end = read_proc_stats(targetd.pid, clock_ticks)
        if adc_end is None:
            adc_end = adc_latest
        if adc_start is not None and adc_end is not None:
            adc_cpu_seconds = max(0.0, adc_end["cpu_seconds"] - adc_start["cpu_seconds"])
        status = run_json(["adc", "recorder", "status"], env, results / f"{name}_status.json")

    resource_status = status.get("resource_status") if status else None
    overhead = status.get("overhead") if status else None
    return {
        "mode": mode,
        "duration_seconds": duration,
        "workload_memory_mb": args.workload_memory_mb,
        "workload_iterations": iterations,
        "workload_iterations_per_second": iterations / duration,
        "workload_checksum": checksum,
        "adc_targetd_cpu_seconds": adc_cpu_seconds,
        "adc_targetd_peak_rss_kb": adc_peak_rss_kb if targetd is not None else None,
        "resource_status": resource_status,
        "overhead": overhead,
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--remote-root", type=pathlib.Path, required=True)
    parser.add_argument("--duration-seconds", type=float, required=True)
    parser.add_argument("--workload-memory-mb", type=int, required=True)
    args = parser.parse_args()

    results = args.remote_root / "results"
    clock_ticks = os.sysconf(os.sysconf_names["SC_CLK_TCK"])
    phases = {
        "baseline": run_phase("baseline", "off", args, clock_ticks),
        "recorder_normal": run_phase("recorder-normal", "normal", args, clock_ticks),
        "recorder_battery_low": run_phase("recorder-battery-low", "battery_low", args, clock_ticks),
    }

    baseline_ips = phases["baseline"]["workload_iterations_per_second"]
    normal_ips = phases["recorder_normal"]["workload_iterations_per_second"]
    battery_ips = phases["recorder_battery_low"]["workload_iterations_per_second"]
    normal_slowdown = max(0.0, 1.0 - normal_ips / baseline_ips) if baseline_ips else 1.0
    battery_slowdown = max(0.0, 1.0 - battery_ips / baseline_ips) if baseline_ips else 1.0

    thresholds = {
        "max_workload_slowdown_ratio": 0.50,
        "max_adc_targetd_cpu_seconds_per_phase": args.duration_seconds * 0.50,
        "max_adc_targetd_peak_rss_kb": 131072,
    }

    def recorder_phase_passed(phase):
        resource = phase["resource_status"] or {}
        return (
            resource.get("continuous_ring_disk_write_bytes") == 0
            and resource.get("frozen_artifact_write_bytes") == 0
            and (phase["adc_targetd_cpu_seconds"] or 0.0) <= thresholds["max_adc_targetd_cpu_seconds_per_phase"]
            and (phase["adc_targetd_peak_rss_kb"] or 0) <= thresholds["max_adc_targetd_peak_rss_kb"]
        )

    passed = (
        normal_slowdown <= thresholds["max_workload_slowdown_ratio"]
        and battery_slowdown <= thresholds["max_workload_slowdown_ratio"]
        and recorder_phase_passed(phases["recorder_normal"])
        and recorder_phase_passed(phases["recorder_battery_low"])
        and phases["recorder_battery_low"]["resource_status"].get("policy_mode") == "battery_low"
    )

    report = {
        "schema_version": "adc.target55_recorder_load_impact_smoke.v1",
        "target": "target55",
        "passed": passed,
        "duration_seconds_per_phase": args.duration_seconds,
        "workload_memory_mb": args.workload_memory_mb,
        "thresholds": thresholds,
        "comparisons": {
            "normal_workload_slowdown_ratio": normal_slowdown,
            "battery_low_workload_slowdown_ratio": battery_slowdown,
        },
        "phases": phases,
    }
    (results / "load_impact_summary.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(report, indent=2))
    if not passed:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
PY

ssh "$HOST" "set -euo pipefail; export PATH='$REMOTE_ROOT/bin':\$PATH; python3 '$REMOTE_ROOT/run_load_impact.py' --remote-root '$REMOTE_ROOT' --duration-seconds '$DURATION_SECONDS' --workload-memory-mb '$WORKLOAD_MEMORY_MB' > '$REMOTE_ROOT/results/load_impact_summary.stdout.json'"

scp -r "$HOST:$REMOTE_ROOT/results/." "$RESULT_ROOT/" >/dev/null

python3 - "$RESULT_ROOT/load_impact_summary.json" <<'PY'
import json
import sys

report = json.load(open(sys.argv[1], "r", encoding="utf-8"))
assert report["schema_version"] == "adc.target55_recorder_load_impact_smoke.v1"
assert report["passed"] is True
assert report["phases"]["recorder_normal"]["resource_status"]["continuous_ring_disk_write_bytes"] == 0
assert report["phases"]["recorder_battery_low"]["resource_status"]["continuous_ring_disk_write_bytes"] == 0
assert report["phases"]["recorder_normal"]["resource_status"]["frozen_artifact_write_bytes"] == 0
assert report["phases"]["recorder_battery_low"]["resource_status"]["frozen_artifact_write_bytes"] == 0
assert report["phases"]["recorder_battery_low"]["resource_status"]["policy_mode"] == "battery_low"
print(json.dumps({
    "schema_version": "adc.target55_recorder_load_impact_smoke_result.v1",
    "passed": True,
    "normal_workload_slowdown_ratio": report["comparisons"]["normal_workload_slowdown_ratio"],
    "battery_low_workload_slowdown_ratio": report["comparisons"]["battery_low_workload_slowdown_ratio"],
    "normal_adc_targetd_cpu_seconds": report["phases"]["recorder_normal"]["adc_targetd_cpu_seconds"],
    "battery_low_adc_targetd_cpu_seconds": report["phases"]["recorder_battery_low"]["adc_targetd_cpu_seconds"],
    "normal_adc_targetd_peak_rss_kb": report["phases"]["recorder_normal"]["adc_targetd_peak_rss_kb"],
    "battery_low_adc_targetd_peak_rss_kb": report["phases"]["recorder_battery_low"]["adc_targetd_peak_rss_kb"],
}, indent=2))
PY

if [[ "$KEEP_REMOTE" -eq 0 ]]; then
  ssh "$HOST" "rm -rf '$REMOTE_ROOT'"
else
  echo "remote smoke root retained: $HOST:$REMOTE_ROOT" >"$RESULT_ROOT/remote_retained.txt"
fi
