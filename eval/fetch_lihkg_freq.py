"""Fetch the LIHKG Cantonese word frequency list used by the ranking prior.

The list is 665,680,302 tokens of Hong Kong forum text segmented into words, and
supplies the whole-word salience signal that character frequency alone cannot
give (see eval/RANKING.md). It is MIT licensed, so unlike the words.hk data it
may be redistributed; it is fetched rather than committed only to keep the
repository small.

Pinned to a commit so a rebuild is reproducible.

    python eval/fetch_lihkg_freq.py

Writes full/lihkg-freq.tsv, which console's build step picks up automatically.
Building without it is supported and simply skips the signal.
"""

import sys
import urllib.request
from pathlib import Path

COMMIT = "56ec4da0963ad1842e755eb1e430df708803c0e2"
URL = (
    "https://raw.githubusercontent.com/AlienKevin/cantonese_frequency_list/"
    f"{COMMIT}/freq.tsv"
)

# Deliberately freq.tsv and not wordhk_freq.tsv from the same repository: that
# variant is filtered to words.hk headwords, which would both reintroduce the
# non-commercial licence and make the signal circular with the attested prior.

DEST = Path(__file__).resolve().parent.parent / "full" / "lihkg-freq.tsv"

EXPECTED_MIN_LINES = 100_000


def main() -> int:
    DEST.parent.mkdir(parents=True, exist_ok=True)

    print(f"Fetching {URL}")
    with urllib.request.urlopen(URL) as response:
        data = response.read().decode("utf-8")

    lines = [line for line in data.splitlines() if line.strip()]
    if len(lines) < EXPECTED_MIN_LINES:
        print(
            f"Refusing to write: got {len(lines)} lines, expected at least "
            f"{EXPECTED_MIN_LINES}. The source may have moved.",
            file=sys.stderr,
        )
        return 1

    total = 0
    for line in lines:
        parts = line.split("\t")
        if len(parts) != 2:
            print(f"Refusing to write: malformed line {line!r}", file=sys.stderr)
            return 1
        total += int(parts[1])

    DEST.write_text("\n".join(lines) + "\n", encoding="utf-8", newline="\n")
    print(f"Wrote {len(lines)} forms ({total} tokens) to {DEST}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
