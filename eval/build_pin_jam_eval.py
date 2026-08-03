"""
Build an evaluation query set for pin-jam (變調, colloquial tone change).

Cantonese systematically raises the final syllable of many words to a high-rising
(tone 2) or high-level (tone 1) tone in speech, while dictionaries record the
formal reading. A learner types what they HEAR, so 英文 must be findable as
"jing1 man2" even though CC-Canto stores it as "jing1 man4".

Method
------
rime-cantonese's word.csv records how words are actually pronounced (sandhi
included). The project's own source data records the formal reading. Where the
two agree on every syllable base but differ only in the tone of the FINAL
syllable, and that tone rises to 1 or 2, we have an attested pin-jam pair.

The query is the spoken form; the expectation is any entry the dictionary stores
under the formal form (which absorbs orthographic variants such as
干炒牛河 / 乾炒牛河).

Usage
-----
    python eval/build_pin_jam_eval.py [--rime-csv PATH] [--limit N]

word.csv is downloaded to full/rime-word.csv if not already present:
https://raw.githubusercontent.com/CanCLID/rime-cantonese-upstream/main/word.csv
"""

import argparse
import csv
import json
import re
import urllib.request
from collections import defaultdict
from pathlib import Path

SCRIPT_DIR = Path(__file__).parent
REPO_DIR = SCRIPT_DIR.parent
OUT_PATH = SCRIPT_DIR / "query_sets" / "pin_jam.json"
RIME_CSV = REPO_DIR / "full" / "rime-word.csv"
RIME_URL = "https://raw.githubusercontent.com/CanCLID/rime-cantonese-upstream/main/word.csv"

SOURCE_FILES = [
    REPO_DIR / "full" / "cccanto-webdist.txt",
    REPO_DIR / "full" / "cccedict-canto-readings-150923.txt",
]

ENTRY_RE = re.compile(r"^(\S+)\s+\S+\s+\[[^\]]*\]\s+\{([^}]*)\}")
SYLLABLE_RE = re.compile(r"^([a-z]+)([1-6])$")

# Tones a final syllable rises TO under pin-jam.
RAISED_TONES = ("1", "2")

ID_START = 6001


def norm(jyutping: str) -> str:
    return " ".join((jyutping or "").lower().split())


def parse_syllables(jyutping: str):
    """Return (bases, tones) or None if any syllable is malformed."""
    bases, tones = [], []
    for syl in norm(jyutping).split():
        m = SYLLABLE_RE.match(syl)
        if not m:
            return None
        bases.append(m.group(1))
        tones.append(m.group(2))
    return (bases, tones) if bases else None


def load_rime(path: Path) -> dict[str, set[str]]:
    if not path.exists():
        print(f"Downloading {RIME_URL}")
        path.parent.mkdir(parents=True, exist_ok=True)
        urllib.request.urlretrieve(RIME_URL, path)

    spoken = defaultdict(set)
    with open(path, encoding="utf-8") as f:
        for row in csv.DictReader(f):
            if row.get("char") and row.get("jyutping"):
                spoken[row["char"]].add(norm(row["jyutping"]))
    print(f"Read {len(spoken)} spoken word forms from {path.name}")
    return spoken


def load_formal() -> tuple[dict[str, set[str]], dict[str, set[str]]]:
    """Returns (characters -> formal jyutpings, formal jyutping -> characters)."""
    by_word = defaultdict(set)
    by_jyutping = defaultdict(set)
    for path in SOURCE_FILES:
        for line in open(path, encoding="utf-8"):
            if not line.strip() or line.startswith("#"):
                continue
            m = ENTRY_RE.match(line)
            if m:
                word, jp = m.group(1), norm(m.group(2))
                by_word[word].add(jp)
                by_jyutping[jp].add(word)
    print(f"Read {len(by_word)} formal word forms from source data")
    return by_word, by_jyutping


def find_pairs(spoken, formal_by_word, formal_by_jyutping):
    pairs = []
    for word, spoken_forms in spoken.items():
        formal_forms = formal_by_word.get(word)
        if not formal_forms:
            continue

        for sp in spoken_forms:
            if sp in formal_forms:
                continue

            sp_parsed = parse_syllables(sp)
            if not sp_parsed:
                continue
            sp_bases, sp_tones = sp_parsed

            for fm in formal_forms:
                fm_parsed = parse_syllables(fm)
                if not fm_parsed:
                    continue
                fm_bases, fm_tones = fm_parsed

                if sp_bases != fm_bases or sp_tones == fm_tones:
                    continue

                diffs = [i for i, (a, b) in enumerate(zip(sp_tones, fm_tones)) if a != b]
                # Canonical pin-jam: exactly one shift, on the final syllable,
                # rising to a high tone from a non-high one.
                if len(diffs) != 1 or diffs[0] != len(sp_tones) - 1:
                    continue
                i = diffs[0]
                if sp_tones[i] not in RAISED_TONES or fm_tones[i] in RAISED_TONES:
                    continue

                pairs.append({
                    "word": word,
                    "spoken": sp,
                    "formal": fm,
                    "shift": f"{fm_tones[i]}->{sp_tones[i]}",
                    "nsyl": len(sp_bases),
                    # Any entry stored under the formal reading is acceptable;
                    # this absorbs orthographic variants (干炒牛河 / 乾炒牛河).
                    "accepted": sorted(formal_by_jyutping.get(fm, {word})),
                })
                break

    return pairs


def to_test_cases(pairs, limit):
    """Sample proportionally across syllable counts so the set reflects the
    real distribution of pin-jam words rather than over-weighting long ones."""
    seen = set()
    unique = []
    for p in sorted(pairs, key=lambda p: (p["nsyl"], p["word"])):
        if p["spoken"] in seen:
            continue
        seen.add(p["spoken"])
        unique.append(p)

    if limit and limit < len(unique):
        by_syl = defaultdict(list)
        for p in unique:
            by_syl[p["nsyl"]].append(p)

        kept = []
        for n, group in sorted(by_syl.items()):
            take = max(1, round(limit * len(group) / len(unique)))
            step = max(1, len(group) // take)
            kept.extend(group[::step][:take])
        unique = sorted(kept, key=lambda p: (p["nsyl"], p["word"]))[:limit]

    cases = []
    for i, p in enumerate(unique):
        cases.append({
            "id": ID_START + i,
            "query": p["spoken"],
            "category": "pin_jam",
            "description": f"{p['word']} - colloquial {p['shift']} tone change of {p['formal']}",
            # Deliberately no expected_jyutping: matching on the formal reading
            # would let any homophone satisfy the case.
            "expected_characters": p["accepted"],
            "accept_top_n": 1 if p["nsyl"] <= 2 else 3,
            "tags": ["pin_jam", f"shift_{p['shift'].replace('->', '_')}", f"syl_{p['nsyl']}"],
        })
    return cases


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--rime-csv", type=Path, default=RIME_CSV)
    ap.add_argument("--limit", type=int, default=300,
                    help="cap the set size (0 = keep all attested pairs)")
    args = ap.parse_args()

    spoken = load_rime(args.rime_csv)
    formal_by_word, formal_by_jyutping = load_formal()
    pairs = find_pairs(spoken, formal_by_word, formal_by_jyutping)
    print(f"Found {len(pairs)} attested pin-jam pairs")

    shifts = defaultdict(int)
    syls = defaultdict(int)
    for p in pairs:
        shifts[p["shift"]] += 1
        syls[p["nsyl"]] += 1
    print(f"  by shift    : {dict(sorted(shifts.items()))}")
    print(f"  by syllables: {dict(sorted(syls.items()))}")

    cases = to_test_cases(pairs, args.limit)
    OUT_PATH.parent.mkdir(parents=True, exist_ok=True)
    with open(OUT_PATH, "w", encoding="utf-8") as f:
        json.dump(cases, f, ensure_ascii=False, indent=2)
    print(f"Wrote {len(cases)} test cases to {OUT_PATH}")


if __name__ == "__main__":
    main()
