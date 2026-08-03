#!/usr/bin/env python3
"""
Jyutping Dictionary Ranking Evaluation Script

Runs queries against the console app and evaluates results against expected values.
Generates a markdown report with positional metrics (p@1, p@2, p@3, MRR),
category breakdowns, baseline diffs, and pattern analysis.
"""

import argparse
import json
import re
import subprocess
import sys
from collections import defaultdict
from datetime import datetime
from pathlib import Path

SCRIPT_DIR = Path(__file__).parent
QUERY_SET_PATH = SCRIPT_DIR / "query_set.json"
QUERY_SETS_DIR = SCRIPT_DIR / "query_sets"
RESULTS_DIR = SCRIPT_DIR / "results"
REPORT_PATH = RESULTS_DIR / "report.md"
BASELINE_PATH = RESULTS_DIR / "baseline.json"
CONSOLE_EXE = SCRIPT_DIR.parent / "console" / "target" / "debug" / "console.exe"
CONSOLE_DIR = SCRIPT_DIR.parent / "console"


def safe_print(*args, **kwargs):
    """Print with UTF-8 encoding fallback."""
    try:
        print(*args, **kwargs)
    except UnicodeEncodeError:
        text = " ".join(str(a) for a in args)
        sys.stdout.buffer.write(text.encode("utf-8", errors="replace"))
        if kwargs.get("end", "\n") == "\n":
            sys.stdout.buffer.write(b"\n")
        sys.stdout.buffer.flush()


def run_batch_queries(queries: list[str], limit: int = 10) -> list[str]:
    """Run all queries in a single batch process and return list of raw outputs (one per query)."""
    safe_print("Start batch...")
    cmd = [str(CONSOLE_EXE), "--batch", "--limit", str(limit)]
    stdin_data = "\n".join(queries) + "\n"
    try:
        result = subprocess.run(
            cmd,
            input=stdin_data,
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
            timeout=300,
            cwd=str(CONSOLE_DIR),
        )
    except subprocess.TimeoutExpired:
        return [f"TIMEOUT (batch)" for _ in queries]
    except Exception as e:
        return [f"ERROR: {e}" for _ in queries]

    safe_print("Done running!")

    blocks = result.stdout.split("===QUERY_END===")
    outputs = []
    for i in range(len(queries)):
        if i < len(blocks):
            outputs.append(blocks[i])
        else:
            outputs.append("")
    if len(blocks) - 1 != len(queries):
        safe_print(f"WARNING: Expected {len(queries)} query blocks but got {len(blocks) - 1}")
    return outputs


def parse_results(output: str) -> list[dict]:
    """Parse the Rust Debug output format into structured results."""
    results = []
    match_blocks = re.split(r"\(Match MatchWithHitInfo", output)

    for block in match_blocks[1:]:
        entry = {}

        term_match = re.search(r"term_match_cost:\s*(\d+)", block)
        unmatched_match = re.search(r"unmatched_position_cost:\s*(\d+)", block)
        inversion_match = re.search(r"inversion_cost:\s*(\d+)", block)
        static_match = re.search(r"static_cost:\s*(\d+)", block)

        if term_match:
            entry["term_match_cost"] = int(term_match.group(1))
        if unmatched_match:
            entry["unmatched_position_cost"] = int(unmatched_match.group(1))
        if inversion_match:
            entry["inversion_cost"] = int(inversion_match.group(1))
        if static_match:
            entry["static_cost"] = int(static_match.group(1))

        entry["total_cost"] = (
            entry.get("term_match_cost", 0)
            + entry.get("unmatched_position_cost", 0)
            + entry.get("inversion_cost", 0)
            + entry.get("static_cost", 0)
        )

        match_type_m = re.search(r"match_type:\s*(Jyutping|Traditional|English)", block)
        if match_type_m:
            entry["match_type"] = match_type_m.group(1)

        entry_id_m = re.search(r"entry_id:\s*(\d+)", block)
        if entry_id_m:
            entry["entry_id"] = int(entry_id_m.group(1))

        chars_m = re.search(r'characters:\s*"([^"]*)"', block)
        if chars_m:
            entry["characters"] = chars_m.group(1)

        jyutping_m = re.search(r'jyutping:\s*"([^"]*)"', block)
        if jyutping_m:
            entry["jyutping"] = jyutping_m.group(1)

        defs_block_m = re.search(
            r"english_definitions:\s*\[(.*?)\]", block, re.DOTALL
        )
        if defs_block_m:
            defs_raw = defs_block_m.group(1)
            defs = re.findall(r'"((?:[^"\\]|\\.)*)"', defs_raw)
            entry["english_definitions"] = defs
        else:
            entry["english_definitions"] = []

        display_cost_m = re.search(r"cost:\s*(\d+),\s*\n\s*entry_source", block)
        if display_cost_m:
            entry["display_cost"] = int(display_cost_m.group(1))

        source_m = re.search(r"entry_source:\s*(CEDict|CCanto)", block)
        if source_m:
            entry["entry_source"] = source_m.group(1)

        if entry.get("characters"):
            results.append(entry)

    return results


def strip_html(text: str) -> str:
    """Remove HTML markup from a string."""
    return re.sub(r"<[^>]+>", "", text)


def _match_result(test_case: dict, r: dict) -> str | None:
    """Check if a single result matches the test case expectations.

    Returns the match type string (e.g. 'character', 'jyutping', 'definition') or None.

    By default `expected_characters` is authoritative: when a case says which
    characters it wants, nothing else can satisfy it. Falling through to
    `expected_jyutping` would compare the result's reading against the expected
    reading, which for a jyutping query is equal for *every homophone* -- so a
    case expecting 講 would be passed by 港, which is the exact failure these
    sets exist to measure.

    A case may set `match_on` to pick a single criterion explicitly. This is for
    sets that assert something other than character identity: `exact_vs_prefix`
    asserts that an exact-reading match outranks a prefix match ("wo1 exact
    should beat wok1/wong1 prefix"), so any result reading `wo1` satisfies it and
    its `expected_characters` are only illustrative.
    """
    r_chars = strip_html(r.get("characters", ""))
    r_jyutping = strip_html(r.get("jyutping", ""))

    expected_chars = test_case.get("expected_characters", [])
    expected_jyutping = test_case.get("expected_jyutping", [])
    definition_contains = test_case.get("definition_contains", [])

    match_on = test_case.get("match_on")
    if match_on == "character":
        expected_jyutping = definition_contains = []
    elif match_on == "jyutping":
        expected_chars = definition_contains = []
    elif match_on == "definition":
        expected_chars = expected_jyutping = []
    elif match_on is not None:
        raise ValueError("test case %s: unknown match_on %r"
                         % (test_case.get("id"), match_on))

    if expected_chars:
        for exp_char in expected_chars:
            if r_chars == exp_char:
                return "character"
        return None

    if expected_jyutping:
        for exp_jp in expected_jyutping:
            if r_jyutping == exp_jp:
                return "jyutping"

    if definition_contains:
        defs = " ".join(r.get("english_definitions", []))
        for kw in definition_contains:
            if kw.lower() in defs.lower():
                return "definition"

    return None


def evaluate_query(test_case: dict, results: list[dict]) -> dict:
    """Evaluate a single query's results against expectations.

    Records positional data: the 1-indexed position of the first matching result
    across ALL returned results (not just top N), or None if not found.
    """
    eval_result = {
        "id": test_case["id"],
        "query": test_case["query"],
        "category": test_case["category"],
        "description": test_case["description"],
        "position": None,
        "found_in_results": False,
        "top_results": [],
        "match_details": None,
    }

    if not results:
        return eval_result

    # Store top 5 results for reporting
    for r in results[:5]:
        eval_result["top_results"].append({
            "characters": strip_html(r.get("characters", "")),
            "jyutping": strip_html(r.get("jyutping", "")),
            "total_cost": r.get("total_cost", 0),
            "static_cost": r.get("static_cost", 0),
            "term_match_cost": r.get("term_match_cost", 0),
            "unmatched_position_cost": r.get("unmatched_position_cost", 0),
            "match_type": r.get("match_type", ""),
            "entry_source": r.get("entry_source", ""),
        })

    # Search ALL results for the first match
    for i, r in enumerate(results):
        match_type = _match_result(test_case, r)
        if match_type is not None:
            position = i + 1
            eval_result["position"] = position
            eval_result["found_in_results"] = True
            eval_result["match_details"] = {
                "matched_on": match_type,
                "matched_value": strip_html(r.get("characters", "")),
                "position": position,
            }
            break

    return eval_result


# --- Metrics computation ---

def compute_metrics(eval_results: list[dict]) -> dict:
    """Compute aggregate metrics from a list of eval results."""
    total = len(eval_results)
    if total == 0:
        return {"count": 0, "p@1": 0, "p@2": 0, "p@3": 0, "mrr": 0, "not_found": 0}

    p1 = sum(1 for r in eval_results if r["position"] is not None and r["position"] <= 1)
    p2 = sum(1 for r in eval_results if r["position"] is not None and r["position"] <= 2)
    p3 = sum(1 for r in eval_results if r["position"] is not None and r["position"] <= 3)
    not_found = sum(1 for r in eval_results if r["position"] is None)

    rr_sum = 0.0
    for r in eval_results:
        if r["position"] is not None:
            rr_sum += 1.0 / r["position"]

    return {
        "count": total,
        "p@1": p1 / total,
        "p@2": p2 / total,
        "p@3": p3 / total,
        "mrr": rr_sum / total,
        "not_found": not_found,
    }


def compute_category_metrics(eval_results: list[dict]) -> dict[str, dict]:
    """Compute metrics grouped by category."""
    by_cat = defaultdict(list)
    for r in eval_results:
        by_cat[r["category"]].append(r)
    return {cat: compute_metrics(results) for cat, results in by_cat.items()}


# --- Baseline diff ---

def load_baseline() -> dict | None:
    """Load baseline results if they exist. Returns dict keyed by test case id."""
    if not BASELINE_PATH.exists():
        return None
    with open(BASELINE_PATH, "r", encoding="utf-8") as f:
        data = json.load(f)
    return {r["id"]: r for r in data["results"]}


def save_baseline(eval_results: list[dict], metrics: dict):
    """Save current results as the baseline."""
    RESULTS_DIR.mkdir(parents=True, exist_ok=True)
    data = {
        "saved_at": datetime.now().isoformat(),
        "metrics": metrics,
        "results": eval_results,
    }
    with open(BASELINE_PATH, "w", encoding="utf-8") as f:
        json.dump(data, f, indent=2, ensure_ascii=False)
    safe_print(f"Baseline saved to: {BASELINE_PATH}")


def compute_diff(eval_results: list[dict], baseline: dict) -> dict:
    """Compare current results against baseline. Returns diff info."""
    improvements = []
    regressions = []
    newly_found = []
    newly_lost = []
    new_cases = []

    for r in eval_results:
        rid = r["id"]
        if rid not in baseline:
            new_cases.append(r)
            continue

        base = baseline[rid]
        base_pos = base.get("position")
        cur_pos = r["position"]

        if base_pos is None and cur_pos is not None:
            newly_found.append({"id": rid, "query": r["query"], "position": cur_pos})
        elif base_pos is not None and cur_pos is None:
            newly_lost.append({"id": rid, "query": r["query"], "old_position": base_pos})
        elif base_pos is not None and cur_pos is not None:
            if cur_pos < base_pos:
                improvements.append({
                    "id": rid, "query": r["query"],
                    "old": base_pos, "new": cur_pos,
                })
            elif cur_pos > base_pos:
                regressions.append({
                    "id": rid, "query": r["query"],
                    "old": base_pos, "new": cur_pos,
                })

    return {
        "improvements": improvements,
        "regressions": regressions,
        "newly_found": newly_found,
        "newly_lost": newly_lost,
        "new_cases": new_cases,
    }


# --- Report generation ---

def _fmt_pct(value: float) -> str:
    return f"{100 * value:.0f}%"


def _fmt_mrr(value: float) -> str:
    return f"{value:.2f}"


def generate_report(eval_results: list[dict], test_cases: list[dict],
                    baseline: dict | None = None) -> str:
    """Generate a markdown report from evaluation results."""
    lines = []
    now = datetime.now().strftime("%Y-%m-%d %H:%M:%S")
    overall = compute_metrics(eval_results)
    cat_metrics = compute_category_metrics(eval_results)

    lines.append("# Dictionary Ranking Evaluation Report")
    lines.append("")
    lines.append(f"Generated: {now}")
    lines.append("")

    # Summary
    lines.append("## Summary")
    lines.append("")
    lines.append(f"- **Total queries**: {overall['count']}")
    lines.append(f"- **p@1**: {_fmt_pct(overall['p@1'])}")
    lines.append(f"- **p@2**: {_fmt_pct(overall['p@2'])}")
    lines.append(f"- **p@3**: {_fmt_pct(overall['p@3'])}")
    lines.append(f"- **MRR**: {_fmt_mrr(overall['mrr'])}")
    lines.append(f"- **Not found**: {overall['not_found']}")
    lines.append("")

    # Category breakdown
    lines.append("## Category Results")
    lines.append("")
    lines.append("| Category | Count | p@1 | p@2 | p@3 | MRR | Not Found |")
    lines.append("|----------|-------|-----|-----|-----|-----|-----------|")

    # Sort categories: known order first, then any extras alphabetically
    cat_order = [
        "single_tone", "single_no_tone", "multi_syllable",
        "cantonese_vocab", "partial_prefix", "exact_vs_prefix",
        "english", "character", "edge_case",
    ]
    sorted_cats = [c for c in cat_order if c in cat_metrics]
    sorted_cats += sorted(c for c in cat_metrics if c not in cat_order)

    for cat in sorted_cats:
        m = cat_metrics[cat]
        lines.append(
            f"| {cat} | {m['count']} | {_fmt_pct(m['p@1'])} | "
            f"{_fmt_pct(m['p@2'])} | {_fmt_pct(m['p@3'])} | "
            f"{_fmt_mrr(m['mrr'])} | {m['not_found']} |"
        )

    # Overall row
    lines.append(
        f"| **overall** | **{overall['count']}** | **{_fmt_pct(overall['p@1'])}** | "
        f"**{_fmt_pct(overall['p@2'])}** | **{_fmt_pct(overall['p@3'])}** | "
        f"**{_fmt_mrr(overall['mrr'])}** | **{overall['not_found']}** |"
    )
    lines.append("")

    # Baseline diff section
    if baseline is not None:
        diff = compute_diff(eval_results, baseline)
        lines.append("## Baseline Comparison")
        lines.append("")

        # Recompute baseline metrics from the baseline data
        base_results = list(baseline.values())
        base_overall = compute_metrics(base_results)

        lines.append("### Aggregate Delta")
        lines.append("")
        for metric_key, label in [("p@1", "p@1"), ("p@2", "p@2"), ("p@3", "p@3"), ("mrr", "MRR")]:
            old_val = base_overall[metric_key]
            new_val = overall[metric_key]
            delta = new_val - old_val
            if metric_key == "mrr":
                lines.append(f"- **{label}**: {_fmt_mrr(old_val)} -> {_fmt_mrr(new_val)} ({'+' if delta >= 0 else ''}{_fmt_mrr(delta)})")
            else:
                delta_pct = delta * 100
                lines.append(f"- **{label}**: {_fmt_pct(old_val)} -> {_fmt_pct(new_val)} ({'+' if delta_pct >= 0 else ''}{delta_pct:.0f}%)")
        lines.append("")

        if diff["improvements"]:
            lines.append(f"### Improvements ({len(diff['improvements'])})")
            lines.append("")
            for d in sorted(diff["improvements"], key=lambda x: x["old"] - x["new"], reverse=True):
                lines.append(f"- #{d['id']} `{d['query']}`: position {d['old']} -> {d['new']}")
            lines.append("")

        if diff["regressions"]:
            lines.append(f"### Regressions ({len(diff['regressions'])})")
            lines.append("")
            for d in sorted(diff["regressions"], key=lambda x: x["new"] - x["old"], reverse=True):
                lines.append(f"- #{d['id']} `{d['query']}`: position {d['old']} -> {d['new']}")
            lines.append("")

        if diff["newly_found"]:
            lines.append(f"### Newly Found ({len(diff['newly_found'])})")
            lines.append("")
            for d in diff["newly_found"]:
                lines.append(f"- #{d['id']} `{d['query']}`: now at position {d['position']}")
            lines.append("")

        if diff["newly_lost"]:
            lines.append(f"### Newly Lost ({len(diff['newly_lost'])})")
            lines.append("")
            for d in diff["newly_lost"]:
                lines.append(f"- #{d['id']} `{d['query']}`: was at position {d['old_position']}, now not found")
            lines.append("")

        if diff["new_cases"]:
            lines.append(f"### New Test Cases ({len(diff['new_cases'])})")
            lines.append("")
            for d in diff["new_cases"]:
                pos_str = str(d["position"]) if d["position"] is not None else "not found"
                lines.append(f"- #{d['id']} `{d['query']}`: position {pos_str}")
            lines.append("")

    # Queries not at position 1 (detailed)
    not_at_1 = [r for r in eval_results if r["position"] is None or r["position"] > 1]
    if not_at_1:
        lines.append(f"## Queries Not at Position 1 ({len(not_at_1)})")
        lines.append("")

        for r in not_at_1:
            pos_str = str(r["position"]) if r["position"] is not None else "not found"
            lines.append(f"### #{r['id']}: `{r['query']}` ({r['category']}) - position: {pos_str}")
            lines.append("")
            lines.append(f"**Description**: {r['description']}")
            lines.append("")

            if r["top_results"]:
                lines.append("**Top results:**")
                lines.append("")
                lines.append(
                    "| # | Characters | Jyutping | Total Cost | Static | Term Match | Unmatched | Type | Source |"
                )
                lines.append(
                    "|---|-----------|----------|------------|--------|------------|-----------|------|--------|"
                )
                for i, tr in enumerate(r["top_results"], 1):
                    lines.append(
                        f"| {i} | {tr['characters']} | {tr['jyutping']} | "
                        f"{tr['total_cost']} | {tr['static_cost']} | "
                        f"{tr['term_match_cost']} | {tr['unmatched_position_cost']} | "
                        f"{tr['match_type']} | {tr['entry_source']} |"
                    )
                lines.append("")

    # Pattern analysis
    lines.append("## Pattern Analysis")
    lines.append("")

    ccanto_wins = 0
    cedict_wins = 0
    for r in eval_results:
        if not r["top_results"]:
            continue
        top = r["top_results"][0]
        if top["entry_source"] == "CCanto":
            ccanto_wins += 1
        elif top["entry_source"] == "CEDict":
            cedict_wins += 1

    lines.append("### Source Distribution in Top Results")
    lines.append("")
    lines.append(f"- CCanto as top result: {ccanto_wins}")
    lines.append(f"- CEDict as top result: {cedict_wins}")
    lines.append("")

    not_at_1_with_results = [r for r in not_at_1 if r["top_results"]]
    if not_at_1_with_results:
        exact_loses = sum(1 for r in not_at_1_with_results if r["top_results"][0]["term_match_cost"] > 0)
        high_static = sum(1 for r in not_at_1_with_results if r["top_results"][0]["static_cost"] > 15000)
        lines.append("### Non-Ideal Result Patterns")
        lines.append("")
        lines.append(f"- Top result has term_match_cost > 0 (partial/fuzzy match winning): {exact_loses}")
        lines.append(f"- Top result has static_cost > 15000 (high base cost): {high_static}")
        lines.append("")

    # All results detail
    lines.append("## All Results")
    lines.append("")
    lines.append("| # | Query | Position | Top Char | Top JP | Total Cost | Source |")
    lines.append("|---|-------|----------|----------|--------|------------|--------|")

    for r in eval_results:
        pos_str = str(r["position"]) if r["position"] is not None else "-"
        if r["top_results"]:
            top = r["top_results"][0]
            lines.append(
                f"| {r['id']} | `{r['query']}` | {pos_str} | "
                f"{top['characters']} | {top['jyutping']} | "
                f"{top['total_cost']} | {top['entry_source']} |"
            )
        else:
            lines.append(
                f"| {r['id']} | `{r['query']}` | {pos_str} | - | - | - | - |"
            )

    lines.append("")
    return "\n".join(lines)


# --- Query set loading ---

def load_query_sets(query_set_path: str | None = None) -> list[dict]:
    """Load test cases from query set files.

    If query_set_path is specified, load only that file.
    Otherwise, merge query_set.json with all files in query_sets/.
    """
    if query_set_path:
        path = Path(query_set_path)
        if not path.exists():
            safe_print(f"ERROR: Query set file not found: {path}")
            sys.exit(1)
        with open(path, "r", encoding="utf-8") as f:
            return json.load(f)

    test_cases = []

    # Load main query_set.json
    if QUERY_SET_PATH.exists():
        with open(QUERY_SET_PATH, "r", encoding="utf-8") as f:
            test_cases.extend(json.load(f))

    # Merge any files from query_sets/
    if QUERY_SETS_DIR.exists():
        for path in sorted(QUERY_SETS_DIR.glob("*.json")):
            with open(path, "r", encoding="utf-8") as f:
                extra = json.load(f)
            safe_print(f"  Loaded {len(extra)} cases from {path.name}")
            test_cases.extend(extra)

    return test_cases


def print_summary(eval_results: list[dict], baseline: dict | None):
    """Print concise summary block to stdout."""
    overall = compute_metrics(eval_results)
    cat_metrics = compute_category_metrics(eval_results)

    safe_print(f"\n{'=' * 60}")
    safe_print(
        f"Results: {overall['count']} queries | "
        f"p@1: {_fmt_pct(overall['p@1'])} | "
        f"p@3: {_fmt_pct(overall['p@3'])} | "
        f"MRR: {_fmt_mrr(overall['mrr'])} | "
        f"not found: {overall['not_found']}"
    )

    # Category breakdown
    cat_order = [
        "single_tone", "single_no_tone", "multi_syllable",
        "cantonese_vocab", "partial_prefix", "exact_vs_prefix",
        "english", "character", "edge_case",
    ]
    sorted_cats = [c for c in cat_order if c in cat_metrics]
    sorted_cats += sorted(c for c in cat_metrics if c not in cat_order)

    safe_print(f"\n{'Category':<20} {'Count':>5} {'p@1':>5} {'p@3':>5} {'MRR':>5} {'Miss':>5}")
    safe_print(f"{'-'*20} {'-'*5} {'-'*5} {'-'*5} {'-'*5} {'-'*5}")
    for cat in sorted_cats:
        m = cat_metrics[cat]
        safe_print(
            f"{cat:<20} {m['count']:>5} "
            f"{_fmt_pct(m['p@1']):>5} {_fmt_pct(m['p@3']):>5} "
            f"{_fmt_mrr(m['mrr']):>5} {m['not_found']:>5}"
        )

    # Baseline delta
    if baseline is not None:
        base_results = list(baseline.values())
        base_overall = compute_metrics(base_results)
        diff = compute_diff(eval_results, baseline)

        d_mrr = overall["mrr"] - base_overall["mrr"]
        d_p1 = (overall["p@1"] - base_overall["p@1"]) * 100
        d_p3 = (overall["p@3"] - base_overall["p@3"]) * 100

        n_improved = len(diff["improvements"]) + len(diff["newly_found"])
        n_regressed = len(diff["regressions"]) + len(diff["newly_lost"])
        n_new = len(diff["new_cases"])

        parts = [
            f"MRR {_fmt_mrr(base_overall['mrr'])} -> {_fmt_mrr(overall['mrr'])} ({'+' if d_mrr >= 0 else ''}{d_mrr:.2f})",
            f"p@1 {_fmt_pct(base_overall['p@1'])} -> {_fmt_pct(overall['p@1'])}",
            f"+{n_improved} improved, -{n_regressed} regressed",
        ]
        if n_new:
            parts.append(f"{n_new} new")

        safe_print(f"\nDelta vs baseline: {' | '.join(parts)}")

        # Print individual regressions
        if diff["regressions"]:
            safe_print(f"\n  Regressions:")
            for d in sorted(diff["regressions"], key=lambda x: x["new"] - x["old"], reverse=True):
                safe_print(f"    #{d['id']} {d['query']}: position {d['old']} -> {d['new']}")
        if diff["newly_lost"]:
            safe_print(f"\n  Newly lost:")
            for d in diff["newly_lost"]:
                safe_print(f"    #{d['id']} {d['query']}: was at position {d['old_position']}")

        # Print improvements
        if diff["improvements"]:
            safe_print(f"\n  Improvements:")
            for d in sorted(diff["improvements"], key=lambda x: x["old"] - x["new"], reverse=True):
                safe_print(f"    #{d['id']} {d['query']}: position {d['old']} -> {d['new']}")
        if diff["newly_found"]:
            safe_print(f"\n  Newly found:")
            for d in diff["newly_found"]:
                safe_print(f"    #{d['id']} {d['query']}: now at position {d['position']}")

    safe_print(f"{'=' * 60}")


def parse_args():
    parser = argparse.ArgumentParser(description="Run dictionary ranking evaluation")
    parser.add_argument("--save-baseline", action="store_true",
                        help="Save current results as the baseline for future comparisons")
    parser.add_argument("--query-set", type=str, default=None,
                        help="Path to a specific query set file (skips merging)")
    parser.add_argument("--tag", type=str, default=None,
                        help="Only run test cases with this tag")
    return parser.parse_args()


def main():
    args = parse_args()

    # Load test cases
    test_cases = load_query_sets(args.query_set)
    safe_print(f"Loaded {len(test_cases)} test cases")

    # Tag filtering
    if args.tag:
        test_cases = [tc for tc in test_cases if args.tag in tc.get("tags", [])]
        safe_print(f"Filtered to {len(test_cases)} cases with tag '{args.tag}'")
        if not test_cases:
            safe_print("No test cases match the given tag.")
            sys.exit(0)

    # Check console exe exists
    if not CONSOLE_EXE.exists():
        safe_print(f"ERROR: Console exe not found at {CONSOLE_EXE}")
        safe_print("Run: cd console && cargo build")
        sys.exit(1)

    # Run all queries in batch mode
    queries = [tc["query"] for tc in test_cases]
    safe_print(f"Running {len(queries)} queries in batch mode...")
    batch_outputs = run_batch_queries(queries, limit=10)
    safe_print(f"Batch complete. Evaluating results...")

    eval_results = []
    for i, tc in enumerate(test_cases):
        raw_output = batch_outputs[i]
        results = parse_results(raw_output)
        eval_result = evaluate_query(tc, results)
        eval_results.append(eval_result)

        # Progress counter (one line, overwritten)
        if (i + 1) % 50 == 0 or i + 1 == len(test_cases):
            safe_print(f"  Evaluated {i + 1}/{len(test_cases)}...")

    # Load baseline if it exists
    baseline = load_baseline()

    # Generate report
    RESULTS_DIR.mkdir(parents=True, exist_ok=True)
    report = generate_report(eval_results, test_cases, baseline)
    with open(REPORT_PATH, "w", encoding="utf-8") as f:
        f.write(report)

    # Print summary
    print_summary(eval_results, baseline)
    safe_print(f"\nReport written to: {REPORT_PATH}")

    # Save raw results
    raw_results_path = RESULTS_DIR / "raw_results.json"
    with open(raw_results_path, "w", encoding="utf-8") as f:
        json.dump(eval_results, f, indent=2, ensure_ascii=False)

    # Save baseline if requested
    if args.save_baseline:
        overall = compute_metrics(eval_results)
        save_baseline(eval_results, overall)


if __name__ == "__main__":
    main()
