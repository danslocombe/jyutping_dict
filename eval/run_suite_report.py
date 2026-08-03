"""Run the complete evaluation suite and report per-query-set metrics.

`run_eval.py` reports one aggregate number over all query sets. That hides
which set moved, which matters a lot here: the sets probe different failure
modes, and a ranking change is expected to move some and leave others flat.
This script breaks the numbers down per set, and can diff two runs so a change
can be A/B'd end to end.

Two details `run_eval.py` gets wrong for whole-suite runs, handled here:

  * it points `CONSOLE_EXE` at the **debug** build, which is slow enough that
    the suite silently trips `run_batch_queries`'s hardcoded `timeout=300` and
    reports 0% accuracy.  This script defaults to the release build and chunks
    the batch so each subprocess call stays well inside the timeout.
  * a timed-out or errored batch is reported as ordinary misses.  Here it is a
    hard failure, so a broken run can never be mistaken for a bad score.

Typical A/B, toggling a search-time constant (no index rebuild needed):

    python eval/run_suite_report.py --save eval/results/on.json
    # ...edit the constant, cargo build --release...
    python eval/run_suite_report.py --save eval/results/off.json
    python eval/run_suite_report.py --compare eval/results/off.json eval/results/on.json

`eval/results/` is already gitignored for generated JSON, so saved runs stay out
of version control.

Building `spoken_corpus.json` additionally needs `pip install -r
eval/requirements.txt`; running the suite does not.

The console resolves its data paths relative to the working directory, but this
script sets `cwd` for the subprocess itself, so it can be run from anywhere.
"""

import argparse
import collections
import json
import pathlib
import sys

EVAL_DIR = pathlib.Path(__file__).resolve().parent
REPO = EVAL_DIR.parent
RELEASE_CONSOLE = REPO / "console" / "target" / "release" / "console.exe"

sys.path.insert(0, str(EVAL_DIR))
import run_eval  # noqa: E402

# Each subprocess call must finish inside run_batch_queries' 300s timeout.
CHUNK_SIZE = 600


def load_suite():
    """All test cases, tagged with the file they came from.

    Mirrors run_eval.load_query_sets (query_set.json plus query_sets/*.json)
    but keeps the provenance, which is the entire point of this script.
    """
    cases = []
    main_set = EVAL_DIR / "query_set.json"
    if main_set.exists():
        for case in json.load(open(main_set, encoding="utf-8")):
            case["_set"] = "query_set (core)"
            cases.append(case)
    for path in sorted((EVAL_DIR / "query_sets").glob("*.json")):
        for case in json.load(open(path, encoding="utf-8")):
            case["_set"] = path.stem
            cases.append(case)
    return cases


def run(console_exe, limit):
    run_eval.CONSOLE_EXE = pathlib.Path(console_exe)
    if not run_eval.CONSOLE_EXE.exists():
        sys.exit("console not found at %s (cargo build --release?)" % console_exe)

    cases = load_suite()
    print("suite: %d cases across %d sets"
          % (len(cases), len({c["_set"] for c in cases})))

    rows = []
    for start in range(0, len(cases), CHUNK_SIZE):
        chunk = cases[start:start + CHUNK_SIZE]
        outputs = run_eval.run_batch_queries([c["query"] for c in chunk], limit=limit)
        for output in outputs:
            if output.startswith(("TIMEOUT", "ERROR")):
                sys.exit("batch failed at offset %d: %s" % (start, output[:120]))
        for case, output in zip(chunk, outputs):
            result = run_eval.evaluate_query(case, run_eval.parse_results(output))
            rows.append({
                "id": case["id"],
                "set": case["_set"],
                "query": case["query"],
                "expected": case.get("expected_characters"),
                "tags": case.get("tags", []),
                "position": result["position"],
            })
    return rows


def metrics(rows):
    n = len(rows)
    hit = [r["position"] for r in rows if r["position"]]
    return {
        "n": n,
        "p1": 100.0 * sum(1 for p in hit if p == 1) / n,
        "p3": 100.0 * sum(1 for p in hit if p <= 3) / n,
        "miss": 100.0 * (n - len(hit)) / n,
        "mrr": sum(1.0 / p for p in hit) / n,
    }


def by_set(rows):
    grouped = collections.defaultdict(list)
    for row in rows:
        grouped[row["set"]].append(row)
    return grouped


def slices(rows):
    """Named subsets worth reporting beyond the per-file split."""
    out = {}
    hard = [r for r in rows if "hard" in r["tags"]]
    if hard:
        out["spoken_corpus:hard"] = hard
    variant = [r for r in rows if "variant_risk" in r["tags"]]
    if variant:
        out["spoken_corpus:variant_risk"] = variant
    return out


def print_table(rows):
    header = "%-30s %6s %8s %8s %8s %7s" % ("set", "n", "p@1", "p@3", "miss", "MRR")
    print("\n" + header)
    print("-" * len(header))
    for name, subset in sorted(by_set(rows).items()):
        m = metrics(subset)
        print("%-30s %6d %7.1f%% %7.1f%% %7.1f%% %7.3f"
              % (name, m["n"], m["p1"], m["p3"], m["miss"], m["mrr"]))
    print("-" * len(header))
    m = metrics(rows)
    print("%-30s %6d %7.1f%% %7.1f%% %7.1f%% %7.3f"
          % ("TOTAL", m["n"], m["p1"], m["p3"], m["miss"], m["mrr"]))
    for name, subset in sorted(slices(rows).items()):
        m = metrics(subset)
        print("%-30s %6d %7.1f%% %7.1f%% %7.1f%% %7.3f"
              % ("  " + name, m["n"], m["p1"], m["p3"], m["miss"], m["mrr"]))


def compare(before_rows, after_rows):
    before = {(r["set"], r["id"]): r for r in before_rows}
    after = {(r["set"], r["id"]): r for r in after_rows}
    shared = sorted(set(before) & set(after))
    if len(shared) != len(before) or len(shared) != len(after):
        print("WARNING: runs cover different cases (%d before, %d after, %d shared)"
              % (len(before), len(after), len(shared)))

    def rank(row):
        # Unranked sorts behind every ranked position.
        return row["position"] if row["position"] else 10 ** 6

    improved, regressed = [], []
    for key in shared:
        delta = rank(after[key]) - rank(before[key])
        if delta < 0:
            improved.append(key)
        elif delta > 0:
            regressed.append(key)

    header = "%-30s %8s %8s %9s" % ("set", "p@1 before", "p@1 after", "delta")
    print("\n" + header)
    print("-" * len(header))
    grouped_before, grouped_after = by_set(before_rows), by_set(after_rows)
    for name in sorted(grouped_before):
        if name not in grouped_after:
            continue
        b, a = metrics(grouped_before[name])["p1"], metrics(grouped_after[name])["p1"]
        print("%-30s %9.1f%% %8.1f%% %+8.1f" % (name, b, a, a - b))
    print("-" * len(header))
    b_all, a_all = metrics(before_rows), metrics(after_rows)
    print("%-30s %9.1f%% %8.1f%% %+8.1f"
          % ("TOTAL", b_all["p1"], a_all["p1"], a_all["p1"] - b_all["p1"]))
    print("%-30s %9.3f  %8.3f  %+8.3f"
          % ("MRR", b_all["mrr"], a_all["mrr"], a_all["mrr"] - b_all["mrr"]))

    net = (sum(1 for k in improved if after[k]["position"] == 1)
           - sum(1 for k in regressed if before[k]["position"] == 1))
    print("\n%d improved, %d regressed, net %+d at p@1"
          % (len(improved), len(regressed), net))

    if regressed:
        print("\nregressions:")
        for key in regressed:
            row = before[key]
            print("  %-24s %-14s %s -> %s"
                  % (row["query"], "".join(row["expected"] or []),
                     row["position"], after[key]["position"]))


def main():
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--console", default=str(RELEASE_CONSOLE),
                        help="console binary (default: the release build)")
    parser.add_argument("--limit", type=int, default=10,
                        help="results requested per query (default: 10)")
    parser.add_argument("--save", metavar="PATH",
                        help="write per-case results here for a later --compare")
    parser.add_argument("--compare", nargs=2, metavar=("BEFORE", "AFTER"),
                        help="diff two saved runs instead of running the suite")
    args = parser.parse_args()

    if args.compare:
        before = json.load(open(args.compare[0], encoding="utf-8"))
        after = json.load(open(args.compare[1], encoding="utf-8"))
        compare(before, after)
        return

    rows = run(args.console, args.limit)
    print_table(rows)

    if args.save:
        path = pathlib.Path(args.save)
        path.parent.mkdir(parents=True, exist_ok=True)
        with open(path, "w", encoding="utf-8") as handle:
            json.dump(rows, handle, ensure_ascii=False, indent=1)
        print("\nsaved %d results to %s" % (len(rows), path))


if __name__ == "__main__":
    main()
