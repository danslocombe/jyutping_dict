"""Backfill `expected_characters` into hk_trip.json.

Why
---
hk_trip's 225 cases carried only `expected_jyutping` and `definition_contains`.
Scoring a jyutping query on `expected_jyutping` compares the result's reading
against the expected reading, which is equal for *every* homophone -- so the set
could not distinguish the right word from any same-sounding wrong one, and its
reported 92% p@1 was measured almost entirely through that hole.

How
---
Each case constrains a reading *and* a definition keyword. Neither identifies a
headword alone, but their intersection usually does. Both constraints are read
from the project's source dictionaries rather than from search output, so the
current ranking cannot leak into the ground truth it will later be judged by.

Resolution routes, tried strongest-first (see `resolve`): exact whole-word
keyword match, a reading carried by exactly one headword, a small hand-checked
table, then long content words. The last is needed because the phrasebook's
English is not the dictionary's -- CEDict glosses 早晨 "early morning" where the
phrasebook says "Good morning".

Keyword matching is word-boundary based, not substring. That matters: 101 of the
224 keywords are single short words, and a substring test for "Hi" matches any
gloss containing "this", which is how a naive version derived 薈 for wai3 "Hi".

Cases that no route resolves are **dropped**. In every such case no source entry
carries the reading at all, so the term is missing from the dictionary or spelled
differently there -- a coverage gap rather than a ranking failure. Scoring them
would measure nothing, and asserting the definition instead manufactures false
failures for words the ranker already returns at rank 1.

Usage
-----
    python eval/backfill_hk_trip.py --dry-run     # report, change nothing
    python eval/backfill_hk_trip.py               # rewrite hk_trip.json

Re-running is idempotent: it reproduces the committed hk_trip.json byte for byte.
"""

import argparse
import collections
import io
import json
import os
import re

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
FULL = os.path.join(REPO, "full")
HK_TRIP = os.path.join(REPO, "eval", "query_sets", "hk_trip.json")

CEDICT_LINE = re.compile(r"^(\S+)\s+(\S+)\s+\[([^\]]*)\]\s+\{([^}]*)\}\s*(/.*)?$")


def load_sources():
    """headword -> (set of readings, concatenated English glosses)."""
    readings = collections.defaultdict(set)
    glosses = collections.defaultdict(list)

    for name in ("cccanto-webdist.txt", "cccedict-canto-readings-150923.txt"):
        for line in io.open(os.path.join(FULL, name), encoding="utf-8"):
            if line.startswith("#"):
                continue
            match = CEDICT_LINE.match(line.rstrip("\n"))
            if not match:
                continue
            head, _simp, _pin, jyut, gloss = match.groups()
            readings[head].add(" ".join(jyut.lower().split()))
            if gloss:
                glosses[head].append(gloss.strip("/").replace("/", " ; "))

    # cedict carries no jyutping but most of the English glosses
    for line in io.open(os.path.join(FULL, "cedict_ts.u8"), encoding="utf-8"):
        if line.startswith("#"):
            continue
        head = line.split(" ", 1)[0]
        parts = line.split("/", 1)
        if len(parts) > 1:
            glosses[head].append(parts[1].strip().strip("/").replace("/", " ; "))

    by_reading = collections.defaultdict(set)
    for head, jset in readings.items():
        for reading in jset:
            by_reading[reading].add(head)
    return by_reading, {h: " ; ".join(g).lower() for h, g in glosses.items()}


def keyword_matches(gloss, keyword):
    """Whole-word match, so "Hi" does not match "this" and "go" does not match
    "goodbye"."""
    pattern = r"(?<![a-z])%s(?![a-z])" % re.escape(keyword.lower().strip())
    return re.search(pattern, gloss) is not None


# Everyday words whose phrasebook gloss shares no wording with the dictionary's
# ("Ok got it" vs 得 "to obtain", "Ten cent denomination" vs 毫 "fine hair").
# Each value is asserted, not invented: `resolve` only accepts an entry here if
# the source dictionaries already list it under the case's own reading, so a
# typo or a wrong guess drops the case rather than passing a bad expectation.
MANUAL = {
    "wai3": ["喂"],
    "zung1": ["中"],
    "me1": ["咩"],
    "dak1": ["得"],
    "dou1": ["都"],
    "cing2": ["請"],
    "hou4": ["毫"],
    "daai6 paai4 dong3": ["大排檔", "大牌檔"],
}


# Function words carry no identifying power, and a phrasebook is full of them.
STOPWORDS = {
    "a", "an", "the", "i", "me", "my", "mine", "you", "your", "yours", "we",
    "us", "our", "he", "she", "it", "its", "they", "them", "this", "that",
    "these", "those", "is", "are", "am", "was", "were", "be", "been", "being",
    "to", "of", "in", "on", "at", "by", "for", "with", "from", "and", "or",
    "but", "do", "does", "did", "can", "could", "would", "should", "will",
    "shall", "may", "might", "have", "has", "had", "not", "no", "yes", "please",
    "very", "too", "so", "here", "there", "what", "how", "when", "where", "who",
    "why", "some", "any", "get", "got", "one", "two",
}


def content_tokens(keyword):
    """Long content words from a phrasebook gloss.

    hk_trip's English comes from a travel phrasebook, not from the dictionary,
    so the two rarely agree verbatim: 早晨 is glossed "early morning" by CEDict
    but the phrasebook says "Good morning". Matching on the content words alone
    bridges that, while the >=4-character floor keeps short function-like words
    from matching everything.
    """
    tokens = re.findall(r"[a-z]+", keyword.lower())
    return [t for t in tokens if len(t) >= 4 and t not in STOPWORDS]


def resolve(case, by_reading, glosses):
    """Return (headwords, route). Routes are tried strongest-first."""
    candidates = set()
    for jyutping in case.get("expected_jyutping", []):
        candidates |= by_reading.get(" ".join(jyutping.lower().split()), set())
    if not candidates:
        return set(), None
    keywords = [k for k in case.get("definition_contains", []) if k.strip()]

    hit = {head for head in candidates
           if any(keyword_matches(glosses.get(head, ""), kw) for kw in keywords)}
    if hit:
        return hit, "keyword"

    # A reading shared by exactly one headword identifies it outright, and needs
    # no help from the gloss.
    if len(candidates) == 1:
        return set(candidates), "unique-reading"

    hit = {head for head in candidates
           if any(keyword_matches(glosses.get(head, ""), token)
                  for kw in keywords for token in content_tokens(kw))}
    if hit:
        return hit, "content-word"

    manual = MANUAL.get(case["query"])
    if manual and set(manual) <= candidates:
        return set(manual), "manual"
    return set(), None


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--show", type=int, default=20,
                        help="how many resolutions to print (default: 20)")
    args = parser.parse_args()

    by_reading, glosses = load_sources()
    cases = json.load(io.open(HK_TRIP, encoding="utf-8"))

    kept, dropped = [], []
    routes = collections.Counter()
    for case in cases:
        hit, route = resolve(case, by_reading, glosses)
        if hit:
            case["expected_characters"] = sorted(hit)
            case.pop("match_on", None)
            routes[route] += 1
            kept.append((case, sorted(hit), route))
        else:
            dropped.append(case)

    print("hk_trip cases            : %d" % len(cases))
    for route, count in routes.most_common():
        print("  resolved via %-11s: %d" % (route, count))
    print("  unresolvable (dropped) : %d" % len(dropped))

    print("\nsample resolutions:")
    for case, hit, route in kept[:args.show]:
        print("  %-22s %-26s -> %-12s %s"
              % (case["query"], str(case.get("definition_contains"))[:26], route, hit))

    print("\ndropped -- no ground truth derivable from source data:")
    for case in dropped[:args.show]:
        print("  %-22s %-26s %s"
              % (case["query"], str(case.get("definition_contains"))[:26],
                 case.get("expected_jyutping")))

    if args.dry_run:
        print("\n--dry-run: no changes written")
        return

    with io.open(HK_TRIP, "w", encoding="utf-8", newline="\n") as handle:
        json.dump([c for c, _, _ in kept], handle, ensure_ascii=False, indent=2)
        handle.write("\n")
    print("\nwrote %s (%d cases)" % (HK_TRIP, len(kept)))


if __name__ == "__main__":
    main()
