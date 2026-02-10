#!/usr/bin/env python3
"""
Benchmark: Compare SLM decision quality across LLM providers.

Compares dry-run, Ollama (local SLM), and API providers for equipment agent
decision-making in port terminal operations.

Usage:
    # Dry-run only (no Ollama required):
    python3 orchestrator/benchmark_slm.py

    # With Ollama:
    python3 orchestrator/benchmark_slm.py --provider ollama --ollama-model llama3:8b

    # Compare all available providers:
    python3 orchestrator/benchmark_slm.py --compare
"""

import argparse
import json
import statistics
import sys
import time

sys.path.insert(0, "orchestrator")
from llm import DryRunProvider, OllamaProvider, LlmProvider, LlmResponse, get_system_prompt


# Benchmark scenarios for equipment agent decisions
SCENARIOS = [
    {
        "name": "crane_lift_sequence",
        "equipment_type": "crane",
        "prompt": (
            "Container MSKU1234567 (20t) needs to be moved from vessel bay 12 to "
            "yard block A row 3. Wind speed is 15 knots from NW. Adjacent crane is "
            "operating in bay 10. What is the lift sequence?"
        ),
        "context": {
            "container_weight_t": 20,
            "wind_speed_knots": 15,
            "wind_direction": "NW",
            "adjacent_crane_bay": 10,
        },
        "expected_keywords": ["lift", "hoist", "lower", "clearance"],
    },
    {
        "name": "crane_priority_decision",
        "equipment_type": "crane",
        "prompt": (
            "Two containers waiting: REEFER MSKU9876543 (perishable, 12t) and "
            "STANDARD TCLU5551234 (general cargo, 25t). Vessel departure in 2 hours. "
            "The reefer needs power within 30 minutes. Which to move first?"
        ),
        "context": {
            "reefer_deadline_min": 30,
            "vessel_departure_hours": 2,
        },
        "expected_keywords": ["reefer", "priority", "perishable", "first"],
    },
    {
        "name": "tractor_route_planning",
        "equipment_type": "tractor",
        "prompt": (
            "Transport container from quay crane QC-3 to yard block D row 7 slot 4. "
            "Lane A is congested (5 min delay). Lane B is clear. Lane C has maintenance "
            "work in progress. Which route?"
        ),
        "context": {
            "origin": "QC-3",
            "destination": "D-7-4",
            "lane_a_delay_min": 5,
            "lane_b_status": "clear",
            "lane_c_status": "maintenance",
        },
        "expected_keywords": ["lane", "route", "b", "clear"],
    },
    {
        "name": "tractor_multi_stop",
        "equipment_type": "tractor",
        "prompt": (
            "You have 3 containers to deliver: "
            "1) MSCU1111 to block A (nearby, low priority) "
            "2) MSCU2222 to block F (far, high priority - vessel loading) "
            "3) MSCU3333 to block C (medium distance, medium priority). "
            "Optimize the delivery order."
        ),
        "context": {
            "containers": [
                {"id": "MSCU1111", "dest": "A", "priority": "low"},
                {"id": "MSCU2222", "dest": "F", "priority": "high"},
                {"id": "MSCU3333", "dest": "C", "priority": "medium"},
            ]
        },
        "expected_keywords": ["order", "priority", "first"],
    },
    {
        "name": "crane_safety_check",
        "equipment_type": "crane",
        "prompt": (
            "Wind gust detected: 45 knots. Currently holding container TCLU8888 "
            "(30t) at 20m height. Vessel is pitching +/- 3 degrees. "
            "What is the safe action?"
        ),
        "context": {
            "wind_gust_knots": 45,
            "container_weight_t": 30,
            "current_height_m": 20,
            "vessel_pitch_deg": 3,
        },
        "expected_keywords": ["stop", "lower", "safe", "wind"],
    },
]


def score_response(response: LlmResponse, scenario: dict) -> dict:
    """Score a response based on keyword matching and latency."""
    text_lower = response.text.lower()
    keywords_found = sum(1 for kw in scenario["expected_keywords"] if kw in text_lower)
    keyword_score = keywords_found / len(scenario["expected_keywords"]) if scenario["expected_keywords"] else 0

    # Actionability: does the response contain clear instructions?
    actionable = any(word in text_lower for word in ["proceed", "execute", "route", "move", "stop", "lower", "hoist", "first", "deliver", "take"])

    return {
        "keyword_score": round(keyword_score, 2),
        "keywords_matched": keywords_found,
        "keywords_total": len(scenario["expected_keywords"]),
        "actionable": actionable,
        "response_length": len(response.text),
        "latency_ms": round(response.latency_ms, 1),
        "is_error": response.text.startswith("ERROR:"),
    }


def run_benchmark(provider: LlmProvider, scenarios: list[dict], runs: int = 1) -> dict:
    """Run all scenarios against a provider."""
    results = []
    latencies = []

    for scenario in scenarios:
        system_prompt = get_system_prompt(scenario["equipment_type"])
        prompt = scenario["prompt"]
        if scenario.get("context"):
            prompt += f"\n\nOperational context:\n{json.dumps(scenario['context'], indent=2)}"

        scenario_results = []
        for _ in range(runs):
            response = provider.generate(prompt, system=system_prompt)
            score = score_response(response, scenario)
            scenario_results.append({
                "response_preview": response.text[:200],
                **score,
            })
            latencies.append(response.latency_ms)

        # Average scores across runs
        avg_keyword = statistics.mean(r["keyword_score"] for r in scenario_results)
        avg_latency = statistics.mean(r["latency_ms"] for r in scenario_results)
        actionable_pct = sum(1 for r in scenario_results if r["actionable"]) / len(scenario_results)

        results.append({
            "scenario": scenario["name"],
            "equipment_type": scenario["equipment_type"],
            "avg_keyword_score": round(avg_keyword, 2),
            "avg_latency_ms": round(avg_latency, 1),
            "actionable_pct": round(actionable_pct, 2),
            "runs": scenario_results,
        })

    overall_keyword = statistics.mean(r["avg_keyword_score"] for r in results)
    overall_latency = statistics.mean(latencies) if latencies else 0
    overall_actionable = statistics.mean(r["actionable_pct"] for r in results)

    return {
        "provider": provider.provider_name(),
        "ready": provider.is_ready(),
        "scenarios": results,
        "summary": {
            "avg_keyword_score": round(overall_keyword, 2),
            "avg_latency_ms": round(overall_latency, 1),
            "actionable_pct": round(overall_actionable, 2),
            "total_decisions": len(scenarios) * runs,
            "meets_latency_target": overall_latency < 2000,  # <2s requirement
        },
    }


def print_report(benchmark: dict):
    """Print a formatted benchmark report."""
    print(f"\n{'='*70}")
    print(f"Provider: {benchmark['provider']}  (ready: {benchmark['ready']})")
    print(f"{'='*70}")

    for scenario in benchmark["scenarios"]:
        print(f"\n  [{scenario['equipment_type']}] {scenario['scenario']}")
        print(f"    Keyword Score: {scenario['avg_keyword_score']:.0%}")
        print(f"    Actionable:    {scenario['actionable_pct']:.0%}")
        print(f"    Avg Latency:   {scenario['avg_latency_ms']:.1f}ms")
        if scenario["runs"]:
            preview = scenario["runs"][0]["response_preview"]
            print(f"    Response:      {preview[:80]}...")

    s = benchmark["summary"]
    print(f"\n  --- Summary ---")
    print(f"  Avg Keyword Score:  {s['avg_keyword_score']:.0%}")
    print(f"  Avg Latency:        {s['avg_latency_ms']:.1f}ms")
    print(f"  Actionable:         {s['actionable_pct']:.0%}")
    print(f"  Meets <2s target:   {'YES' if s['meets_latency_target'] else 'NO'}")


def main():
    parser = argparse.ArgumentParser(description="Benchmark SLM providers for equipment agents")
    parser.add_argument("--provider", type=str, default="dry-run",
                        choices=["dry-run", "ollama"],
                        help="Provider to benchmark (default: dry-run)")
    parser.add_argument("--ollama-endpoint", type=str, default="http://localhost:11434")
    parser.add_argument("--ollama-model", type=str, default="llama3:8b")
    parser.add_argument("--runs", type=int, default=1,
                        help="Number of runs per scenario (default: 1)")
    parser.add_argument("--compare", action="store_true",
                        help="Compare all available providers")
    parser.add_argument("--json", action="store_true",
                        help="Output results as JSON")
    args = parser.parse_args()

    providers: list[LlmProvider] = []

    if args.compare:
        providers.append(DryRunProvider())
        ollama = OllamaProvider(endpoint=args.ollama_endpoint, model=args.ollama_model)
        if ollama.is_ready():
            providers.append(ollama)
        else:
            print(f"Note: Ollama not available at {args.ollama_endpoint}, skipping.")
    elif args.provider == "dry-run":
        providers.append(DryRunProvider())
    elif args.provider == "ollama":
        providers.append(OllamaProvider(endpoint=args.ollama_endpoint, model=args.ollama_model))

    all_results = []
    for provider in providers:
        result = run_benchmark(provider, SCENARIOS, runs=args.runs)
        all_results.append(result)

    if args.json:
        print(json.dumps(all_results, indent=2))
    else:
        for result in all_results:
            print_report(result)

        if len(all_results) > 1:
            print(f"\n{'='*70}")
            print("COMPARISON")
            print(f"{'='*70}")
            print(f"{'Provider':<15} {'Keyword':<10} {'Latency':<12} {'Actionable':<12} {'<2s?':<6}")
            print(f"{'-'*55}")
            for r in all_results:
                s = r["summary"]
                print(f"{r['provider']:<15} {s['avg_keyword_score']:.0%}{'':<6} "
                      f"{s['avg_latency_ms']:.1f}ms{'':<5} "
                      f"{s['actionable_pct']:.0%}{'':<8} "
                      f"{'YES' if s['meets_latency_target'] else 'NO'}")


if __name__ == "__main__":
    main()
