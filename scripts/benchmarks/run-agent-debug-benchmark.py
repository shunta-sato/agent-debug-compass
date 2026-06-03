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
    results = []
    for scenario in scenarios:
        result = score_scenario(scenario)
        results.append(result)
        for key in metrics:
            metrics[key] += result["metrics"][key]

    report = {
        "schema_version": "obs.agent_debug_benchmark_report.v1",
        "scenario_count": len(scenarios),
        "scenario_ids": [scenario["scenario_id"] for scenario in scenarios],
        "metrics": metrics,
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


if __name__ == "__main__":
    raise SystemExit(main())
