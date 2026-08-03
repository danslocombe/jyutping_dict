"""
Extract a plain headword list from the words.hk CSV dump for use as a
"real word" prior in the dictionary builder.

Frequency data can't distinguish a word Cantonese speakers actually use from a
Classical/Mandarin term that happens to be frequent in a written corpus. That is
why 畢昇 (an 11th-century printer) outranks 不勝, and 惝恍 outranks 劏房.
words.hk is a hand-curated Cantonese dictionary, so mere membership is a strong
signal that a learner might plausibly be searching for the word.

Input is the `all` CSV from https://words.hk/faiman/analysis/wordslist/ .

  ** LICENCE **
  words.hk is (c) Hong Kong Lexicography Limited, released under the
  Non-Commercial Open Data License, NOT public domain. Only entries marked
  已公開 are covered. --open-only (the default) keeps just those.
  See https://words.hk/base/hoifong/ . The generated list and the source CSVs
  are gitignored; do not redistribute without following the licence terms.

Usage:
    python eval/build_wordshk_headwords.py [--all-entries] [--include-unreviewed]
"""

import argparse
import csv
import gzip
from pathlib import Path

REPO_DIR = Path(__file__).parent.parent
SRC = REPO_DIR / "full" / "wordshk-all.csv.gz"
OUT = REPO_DIR / "full" / "wordshk-headwords.txt"

OPEN_STATUS = "已公開"
REVIEWED_STATUS = "OK"

csv.field_size_limit(10_000_000)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--src", type=Path, default=SRC)
    ap.add_argument("--out", type=Path, default=OUT)
    ap.add_argument("--all-entries", action="store_true",
                    help="include entries not marked 已公開 (check the licence first)")
    ap.add_argument("--reviewed-only", action="store_true",
                    help="drop entries flagged as unreviewed")
    args = ap.parse_args()

    opener = gzip.open if args.src.suffix == ".gz" else open
    with opener(args.src, "rt", encoding="utf-8", newline="") as f:
        rows = [r for r in csv.reader(f) if len(r) == 6 and r[0].strip().isdigit()]
    print(f"Read {len(rows)} entries from {args.src.name}")

    words = set()
    skipped_licence = skipped_review = 0
    for _id, head, body, _variant, review, licence in rows:
        if not args.all_entries and licence.strip() != OPEN_STATUS:
            skipped_licence += 1
            continue
        if args.reviewed_only and review.strip() != REVIEWED_STATUS:
            skipped_review += 1
            continue
        if "未有內容" in body or "NO DATA" in body:
            continue
        for part in head.split(","):
            word = part.split(":", 1)[0].strip() if ":" in part else part.strip()
            # Multi-word phrases are out of scope; the ranking problem is terms.
            if word and all("\u3400" <= c <= "\u9fff" or c.isalnum() for c in word):
                words.add(word)

    print(f"  skipped (not {OPEN_STATUS}): {skipped_licence}")
    if args.reviewed_only:
        print(f"  skipped (unreviewed)    : {skipped_review}")

    args.out.parent.mkdir(parents=True, exist_ok=True)
    with open(args.out, "w", encoding="utf-8") as f:
        for w in sorted(words):
            f.write(w + "\n")
    print(f"Wrote {len(words)} headwords to {args.out}")


if __name__ == "__main__":
    main()
