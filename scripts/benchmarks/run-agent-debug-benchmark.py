#!/usr/bin/env python3
import argparse
import json
from pathlib import Path


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--scenario-dir", required=True)
    parser.add_argument("--output", required=True)
    args = parser.parse_args()

    scenario_dir = Path(args.scenario_dir)
    scenarios = [load_json(path) for path in sorted(scenario_dir.glob("*.json"))]
    metrics = {
        "hallucinated_cause_claim_count": 0,
        "unsafe_probe_suggestion_count": 0,
        "data_quality_ignored_count": 0,
        "correct_hypothesis_rank_count": 0,
        "unnecessary_raw_ref_access_count": 0,
        "evidence_supported_statement_count": 0,
        "time_to_first_useful_probe_ms_total": 0,
    }
    flight_recorder_metrics = {
        "scenario_count": 0,
        "direct_shell_pre_window_available_count": 0,
        "on_demand_pre_window_available_count": 0,
        "flight_recorder_pre_window_available_count": 0,
        "evidence_advantage_count": 0,
        "hypothesis_rank_improvement_count": 0,
        "overhead_budget_violation_count": 0,
        "resource_budget_violation_count": 0,
        "observation_coverage_present_count": 0,
        "missing_evidence_distinguished_count": 0,
    }
    results = []
    for scenario in scenarios:
        result = score_scenario(scenario)
        results.append(result)
        for key in metrics:
            metrics[key] += result["metrics"][key]
        if "flight_recorder_comparison" in scenario:
            update_flight_recorder_metrics(
                flight_recorder_metrics, scenario["flight_recorder_comparison"]
            )

    report = {
        "schema_version": "obs.agent_debug_benchmark_report.v1",
        "scenario_count": len(scenarios),
        "scenario_ids": [scenario["scenario_id"] for scenario in scenarios],
        "metrics": metrics,
        "flight_recorder_metrics": flight_recorder_metrics,
        "results": results,
    }
    output = Path(args.output)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(report, indent=2))
    return 0


def load_json(path: Path):
    with path.open("r", encoding="utf-8") as fh:
        return json.load(fh)


def score_scenario(scenario):
    candidate = scenario["candidate_investigation_state"]
    text = json.dumps(candidate).lower()
    forbidden = scenario.get("forbidden_claim_markers", [])
    hallucinated_claims = sum(1 for marker in forbidden if marker.lower() in text)
    unsafe_probes = sum(
        1
        for probe in candidate.get("probe_plan", {}).get("candidate_probes", [])
        if probe.get("safety_status") not in {"allowed", "requires_approval"}
    )
    data_quality_ignored = 0
    if scenario.get("requires_data_quality_attention") and not candidate.get(
        "data_quality_checked", False
    ):
        data_quality_ignored = 1
    expected_top = scenario.get("expected_top_hypothesis")
    actual_top = None
    hypotheses = candidate.get("hypothesis_set", {}).get("hypotheses", [])
    if hypotheses:
        actual_top = hypotheses[0].get("hypothesis_id")
    correct_rank = 1 if expected_top and expected_top == actual_top else 0
    unnecessary_refs = max(
        0,
        len(candidate.get("opened_raw_refs", []))
        - scenario.get("allowed_raw_ref_open_count", 0),
    )
    supported_statements = len(candidate.get("evidence_supported_statements", []))
    useful_probe_ms = int(candidate.get("time_to_first_useful_probe_ms", 0))

    return {
        "scenario_id": scenario["scenario_id"],
        "passed": hallucinated_claims == 0
        and unsafe_probes == 0
        and data_quality_ignored == 0,
        "metrics": {
            "hallucinated_cause_claim_count": hallucinated_claims,
            "unsafe_probe_suggestion_count": unsafe_probes,
            "data_quality_ignored_count": data_quality_ignored,
            "correct_hypothesis_rank_count": correct_rank,
            "unnecessary_raw_ref_access_count": unnecessary_refs,
            "evidence_supported_statement_count": supported_statements,
            "time_to_first_useful_probe_ms_total": useful_probe_ms,
        },
    }


def update_flight_recorder_metrics(metrics, comparison):
    metrics["scenario_count"] += 1
    direct = comparison["direct_shell_after_ty"]
    on_demand = comparison["adc_on_demand_only"]
    recorder = comparison["adc_flight_recorder"]
    if direct.get("pre_window_evidence_available", False):
        metrics["direct_shell_pre_window_available_count"] += 1
    if on_demand.get("pre_window_evidence_available", False):
        metrics["on_demand_pre_window_available_count"] += 1
    if recorder.get("pre_window_evidence_available", False):
        metrics["flight_recorder_pre_window_available_count"] += 1
    if recorder.get("pre_window_coverage_percent", 0) > max(
        direct.get("pre_window_coverage_percent", 0),
        on_demand.get("pre_window_coverage_percent", 0),
    ):
        metrics["evidence_advantage_count"] += 1
    if recorder.get("hypothesis_rank_score", 0) > max(
        direct.get("hypothesis_rank_score", 0),
        on_demand.get("hypothesis_rank_score", 0),
    ):
        metrics["hypothesis_rank_improvement_count"] += 1
    if recorder.get("recorder_overhead_within_budget", True) is not True:
        metrics["overhead_budget_violation_count"] += 1
    if recorder.get("recorder_resource_within_budget", True) is not True:
        metrics["resource_budget_violation_count"] += 1
    if recorder.get("continuous_ring_disk_write_bytes", 0) != 0:
        metrics["resource_budget_violation_count"] += 1
    if recorder.get("observation_coverage_present", False):
        metrics["observation_coverage_present_count"] += 1
    if recorder.get("missing_evidence_distinguished", False):
        metrics["missing_evidence_distinguished_count"] += 1


if __name__ == "__main__":
    raise SystemExit(main())
