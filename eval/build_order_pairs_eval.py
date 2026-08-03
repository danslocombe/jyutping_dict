"""Build `order_pairs.json`: does syllable order decide the ranking?

Why
---
Searching `jat1 maan6` returns 萬一 (maan6 jat1) above 一萬 (jat1 maan6), even
though the second is the exact reading typed. `zau6 gam2` returns 噉就 above
就噉. The ranker does charge for this -- `cost_inversions` adds
OUT_OF_ORDER_INVERSION_PENALTY per inverted pair -- but the penalty is 8,000 and
WORDSHK_ATTESTED_BONUS is also 8,000, so an attested out-of-order entry ties an
unattested in-order one and wins on tie-break.

No existing query set covers this: the bug was found by hand, and the suite
scored it as a pass. This set makes it measurable.

How
---
The dictionary supplies its own ground truth. Any two headwords whose readings
are permutations of each other (一萬/萬一) form a case: query one reading and the
headword carrying *that* order must rank first. No external data is involved, so
the set cannot be circular with any frequency prior we later adopt, and no
judgement call is being made -- if the user types the syllables in an order the
dictionary itself records as a distinct word, that word is the answer.

Only entry-creating sources count. `cccedict-canto-readings-150923.txt` is a
reading-annotation table (`builder.annotate`), not an entry source, so headwords
appearing only there -- 幾百 is one -- are absent from the index and would make
unpassable cases.

Cases are emitted only when *every* ordering in a group is a real entry, so a
failure always means misranking and never a coverage gap.

Usage
-----
    python eval/build_order_pairs_eval.py --dry-run
    python eval/build_order_pairs_eval.py
"""

import argparse
import collections
import io
import json
import os
import re

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
FULL = os.path.join(REPO, "full")
OUT = os.path.join(REPO, "eval", "query_sets", "order_pairs.json")

FIRST_ID = 9001

CCANTO_LINE = re.compile(r"^(\S+)\s+(\S+)\s+\[([^\]]*)\]\s+\{([^}]*)\}\s*(/.*)?$")
READING_LINE = re.compile(r"^(\S+)\s+(\S+)\s+\[([^\]]*)\]\s+\{([^}]*)\}\s*$")


SYLLABLE = re.compile(r"^[a-z]+[1-6]$")


def norm(jyutping):
    return " ".join(jyutping.lower().split())


def is_clean_reading(syllables):
    """Reject readings carrying punctuation.

    Some multi-clause idioms record their reading with the clause comma inline
    ("zi1 gei2 zi1 bei2 ，baak3 zin3 bat1 toi5"). The comma survives tokenization
    as its own term and the query can never match, so such a case would report a
    permanent miss that has nothing to do with ordering.
    """
    return bool(syllables) and all(SYLLABLE.match(s) for s in syllables)


def load_entries():
    """headword -> set of readings, for headwords the builder actually indexes.

    Mirrors console/src/main.rs: cedict_ts.u8 and cccanto-webdist.txt create
    entries; cccedict-canto-readings only annotates existing ones.
    """
    cedict_heads = set()
    for line in io.open(os.path.join(FULL, "cedict_ts.u8"), encoding="utf-8"):
        if not line.startswith("#"):
            cedict_heads.add(line.split(" ", 1)[0])

    readings = collections.defaultdict(set)
    for line in io.open(os.path.join(FULL, "cccanto-webdist.txt"), encoding="utf-8"):
        if line.startswith("#"):
            continue
        match = CCANTO_LINE.match(line.rstrip("\n"))
        if match:
            readings[match.group(1)].add(norm(match.group(4)))

    # Readings for cedict headwords come from the annotation table.
    for line in io.open(os.path.join(FULL, "cccedict-canto-readings-150923.txt"),
                        encoding="utf-8"):
        if line.startswith("#"):
            continue
        match = READING_LINE.match(line.rstrip("\n"))
        if match and match.group(1) in cedict_heads:
            readings[match.group(1)].add(norm(match.group(4)))

    return readings


def build_cases(readings, min_syllables, max_groups):
    # Group headwords by the multiset of syllables in a reading.
    groups = collections.defaultdict(lambda: collections.defaultdict(set))
    for head, reading_set in readings.items():
        for reading in reading_set:
            syllables = reading.split()
            if len(syllables) < min_syllables or len(set(syllables)) < 2:
                continue
            if not is_clean_reading(syllables):
                continue
            groups[tuple(sorted(syllables))][reading].add(head)

    usable = [key for key in sorted(groups) if len(groups[key]) >= 2]
    total_groups = len(usable)
    if max_groups and total_groups > max_groups:
        # Even stride over the sorted keys: deterministic, reproducible, and it
        # samples the whole alphabet rather than a prefix of it. Groups are kept
        # whole so both directions of a pair are always present -- testing only
        # the easy direction would flatter any ordering change.
        stride = total_groups / float(max_groups)
        usable = [usable[int(i * stride)] for i in range(max_groups)]

    cases, next_id = [], FIRST_ID
    for key in usable:
        by_order = groups[key]
        for reading in sorted(by_order):
            others = sorted(set(by_order) - {reading})
            rivals = sorted({h for o in others for h in by_order[o]})
            cases.append({
                "id": next_id,
                "query": reading,
                "category": "order_pairs",
                "description": "%s in this order is %s; competing orders %s"
                               % (reading, "/".join(sorted(by_order[reading])),
                                  ", ".join("%s (%s)" % ("/".join(sorted(by_order[o])), o)
                                            for o in others)),
                "expected_characters": sorted(by_order[reading]),
                "expected_jyutping": [reading],
                "match_on": "character",
                "accept_top_n": 1,
                "tags": ["order_pairs"] + (["permuted_rival"] if rivals else []),
            })
            next_id += 1
    return cases, total_groups


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--min-syllables", type=int, default=2)
    parser.add_argument("--max-groups", type=int, default=200,
                        help="cap on permutation groups sampled (0 = all). "
                             "Both orderings of a group are always kept.")
    parser.add_argument("--show", type=int, default=15)
    args = parser.parse_args()

    readings = load_entries()
    cases, total_groups = build_cases(readings, args.min_syllables, args.max_groups)

    print("indexed headwords with readings: %d" % len(readings))
    print("permutation groups available   : %d" % total_groups)
    print("order_pairs cases              : %d" % len(cases))
    lengths = collections.Counter(len(c["query"].split()) for c in cases)
    for length in sorted(lengths):
        print("  %d syllables: %d" % (length, lengths[length]))

    print("\nsample:")
    for case in cases[:args.show]:
        print("  %-26s -> %-12s | %s"
              % (case["query"], "/".join(case["expected_characters"]),
                 case["description"][:72]))

    if args.dry_run:
        print("\n--dry-run: no changes written")
        return

    with io.open(OUT, "w", encoding="utf-8", newline="\n") as handle:
        json.dump(cases, handle, ensure_ascii=False, indent=2)
        handle.write("\n")
    print("\nwrote %s (%d cases)" % (OUT, len(cases)))


if __name__ == "__main__":
    main()
