#!/usr/bin/env python3
"""
Measure the effect of the words.hk attested-word prior on ranking.

Character frequency cannot tell an everyday Cantonese word from an obscure
Classical/Mandarin term that happens to be frequent in written corpora, so
entries like 畢昇 (an 11th-century printer) outrank 不勝. Entries whose
traditional form appears in words.hk are flagged at build time, and the search
discounts them by WORDSHK_ATTESTED_BONUS.

The bonus applies only where the query accounts for the entry in full. Static
cost carries both salience and length, so an unconditional discount lets longer
attested words displace shorter exact matches - an earlier version of this
experiment applied the discount in builder.rs and regressed 天, 火 and 目 from
rank 1 to unranked.

For each WORDSHK_ATTESTED_BONUS value:
  1. Patch search.rs
  2. cargo build --release
  3. Run the full eval suite

The index is built once up front; the flag is data and the bonus is applied at
search time, so it does not need rebuilding per step.

Bonus 0 is the baseline (prior disabled).

Usage:
  python eval/experiments/wordshk_prior/sweep.py
"""

import json
import re
import shutil
import subprocess
import sys
import time
from datetime import datetime
from pathlib import Path

EXPERIMENT_DIR = Path(__file__).parent
EVAL_DIR = EXPERIMENT_DIR.parent.parent
PROJECT_DIR = EVAL_DIR.parent

BUILDER_RS = PROJECT_DIR / "dictlib" / "src" / "builder.rs"
SEARCH_RS = PROJECT_DIR / "dictlib" / "src" / "search.rs"
CONSOLE_DIR = PROJECT_DIR / "console"
DICT_PATH = PROJECT_DIR / "full" / "full.jyp_dict"
RELEASE_EXE = CONSOLE_DIR / "target" / "release" / "console.exe"
HEADWORDS = PROJECT_DIR / "full" / "wordshk-headwords.txt"
SAVED_DICT_PATH = EXPERIMENT_DIR / "pre_sweep.jyp_dict"

sys.path.insert(0, str(EVAL_DIR))
from run_eval import (
    parse_results, evaluate_query, compute_metrics,
    compute_category_metrics, load_query_sets, safe_print, _fmt_pct, _fmt_mrr,
)

DISCOUNT_VALUES = [0, 2_000, 4_000, 6_000, 8_000, 12_000, 16_000, 20_000]


def patch_discount(content, value):
    return re.sub(
        r"pub const WORDSHK_ATTESTED_BONUS:\s*u32\s*=\s*[\d_]+;",
        f"pub const WORDSHK_ATTESTED_BONUS: u32 = {value};",
        content,
    )


def build_console():
    r = subprocess.run(["cargo", "build", "--release"], capture_output=True,
                       text=True, cwd=str(CONSOLE_DIR), timeout=600)
    if r.returncode != 0:
        safe_print(f"BUILD FAILED:\n{r.stderr[-500:]}")
        return False
    return True


def build_dictionary():
    r = subprocess.run([str(RELEASE_EXE), "build"], capture_output=True, text=True,
                       encoding="utf-8", errors="replace", cwd=str(CONSOLE_DIR), timeout=600)
    if r.returncode != 0:
        safe_print(f"DICT BUILD FAILED:\n{r.stderr[-500:]}")
        return False
    for line in r.stdout.splitlines():
        if "Discounted" in line or "Writing done" in line:
            safe_print(f"  {line}")
    return True


def run_batch_queries_release(queries, limit=10):
    try:
        r = subprocess.run([str(RELEASE_EXE), "--batch", "--limit", str(limit)],
                           input="\n".join(queries) + "\n", capture_output=True, text=True,
                           encoding="utf-8", errors="replace", timeout=600, cwd=str(CONSOLE_DIR))
    except subprocess.TimeoutExpired:
        return ["TIMEOUT (batch)" for _ in queries]
    blocks = r.stdout.split("===QUERY_END===")
    return [blocks[i] if i < len(blocks) else "" for i in range(len(queries))]


def do_eval(test_cases):
    outputs = run_batch_queries_release([tc["query"] for tc in test_cases], limit=10)
    results = [evaluate_query(tc, parse_results(outputs[i])) for i, tc in enumerate(test_cases)]
    return results, compute_metrics(results), compute_category_metrics(results)


def main():
    if not HEADWORDS.exists():
        safe_print(f"Missing {HEADWORDS}. Run eval/build_wordshk_headwords.py first.")
        return

    test_cases = load_query_sets()
    safe_print(f"Loaded {len(test_cases)} test cases")
    safe_print(f"Sweeping WORDSHK_ATTESTED_BONUS over {DISCOUNT_VALUES}")

    original_source = SEARCH_RS.read_text(encoding="utf-8")
    if DICT_PATH.exists():
        shutil.copy2(DICT_PATH, SAVED_DICT_PATH)

    # The attested flag is baked into the index, but the bonus is applied at
    # search time, so the index only needs building once.
    safe_print("\nBuilding index once (attested flags)...")
    if not build_console() or not build_dictionary():
        safe_print("Initial build failed, aborting")
        return

    results_log = []
    per_query = {}

    try:
        for i, value in enumerate(DISCOUNT_VALUES):
            safe_print(f"\n{'='*60}")
            safe_print(f"[{i+1}/{len(DISCOUNT_VALUES)}] WORDSHK_ATTESTED_BONUS={value}")
            safe_print(f"{'='*60}")

            SEARCH_RS.write_text(patch_discount(original_source, value), encoding="utf-8")

            safe_print("Building (release)...")
            t0 = time.time()
            if not build_console():
                results_log.append({"discount": value, "error": "build_failed"})
                continue
            safe_print(f"Built in {time.time()-t0:.1f}s")

            safe_print("Evaluating...")
            t0 = time.time()
            eval_results, overall, cat_metrics = do_eval(test_cases)
            safe_print(f"Evaluated in {time.time()-t0:.1f}s")

            per_query[value] = {r["id"]: r["position"] for r in eval_results}

            entry = {
                "discount": value,
                "overall_p1": overall["p@1"],
                "overall_p3": overall["p@3"],
                "overall_mrr": overall["mrr"],
                "overall_not_found": overall["not_found"],
                "categories": {
                    c: {"p1": m["p@1"], "p3": m["p@3"], "mrr": m["mrr"],
                        "not_found": m["not_found"], "count": m["count"]}
                    for c, m in cat_metrics.items()
                },
            }
            results_log.append(entry)
            safe_print(f"  Overall: p@1={_fmt_pct(overall['p@1'])} p@3={_fmt_pct(overall['p@3'])} "
                       f"MRR={_fmt_mrr(overall['mrr'])} miss={overall['not_found']}")

    finally:
        safe_print("\nRestoring original search.rs...")
        SEARCH_RS.write_text(original_source, encoding="utf-8")
        build_console()
        # Rebuild rather than restoring the pre-sweep index: that copy predates
        # the attested flags, so restoring it would silently disable the prior.
        build_dictionary()
        safe_print("Restored.")

    with open(EXPERIMENT_DIR / "sweep_results.json", "w", encoding="utf-8") as f:
        json.dump({"timestamp": datetime.now().isoformat(),
                   "discount_values": DISCOUNT_VALUES,
                   "results": results_log,
                   "per_query_positions": {str(k): v for k, v in per_query.items()}},
                  f, indent=2)

    cats = sorted({c for r in results_log if "error" not in r for c in r["categories"]})
    safe_print(f"\n{'='*100}")
    safe_print("SWEEP SUMMARY")
    safe_print(f"{'='*100}")
    header = f"{'Disc':>6} | {'p@1':>6} {'p@3':>6} {'MRR':>6} {'Miss':>5} | " + " ".join(f"{c[:11]:>11}" for c in cats)
    safe_print(header)
    safe_print("-" * len(header))
    for r in results_log:
        if "error" in r:
            safe_print(f"{r['discount']:>6} | ERROR")
            continue
        row = (f"{r['discount']:>6} | {_fmt_pct(r['overall_p1']):>6} {_fmt_pct(r['overall_p3']):>6} "
               f"{_fmt_mrr(r['overall_mrr']):>6} {r['overall_not_found']:>5} | ")
        row += " ".join(f"{_fmt_pct(r['categories'].get(c, {}).get('p1', 0)):>11}" for c in cats)
        safe_print(row)

    valid = [r for r in results_log if "error" not in r]
    if valid:
        base = next((r for r in valid if r["discount"] == 0), None)
        best = max(valid, key=lambda r: r["overall_mrr"])
        safe_print(f"\nBest overall MRR: discount={best['discount']} -> MRR={_fmt_mrr(best['overall_mrr'])} "
                   f"p@1={_fmt_pct(best['overall_p1'])}")
        if base:
            safe_print(f"Baseline (0)    : MRR={_fmt_mrr(base['overall_mrr'])} p@1={_fmt_pct(base['overall_p1'])}")


if __name__ == "__main__":
    main()
