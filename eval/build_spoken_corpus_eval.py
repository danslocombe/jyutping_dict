"""Build the `spoken_corpus` evaluation set from HKCanCor.

Why this set exists
-------------------
The existing query sets are either hand-authored or derived from data that now
feeds the ranker itself (words.hk).  Validating a ranking change against the
signal that drives it is circular.  This set is built from HKCanCor, a corpus
that is completely independent of every ranking signal in the project:

  * HKCanCor: 30 hours of spontaneous Cantonese conversation recorded 1997-98,
    hand-transcribed, hand-segmented and hand-romanised.
    Licence: CC BY 4.0.  Distributed via the `pycantonese` package.
    Luke & Wong (2015); Lee, Chen & Tsui (2022).

It is also a more faithful model of the user's actual complaint than a
homophone-pair set: every case is a word a real person actually said, looked up
by the jyutping they actually said it with.  A word that people say hundreds of
times per 150k tokens has to beat the thousands of rare dictionary entries that
share its reading.

Guarantees
----------
Every emitted case is a *ranking* test, never a coverage test:

  * the headword exists in the project's own Cantonese source files, and
  * the queried reading is one those files record for that headword.

Cases failing either check are dropped, so a failure here can only ever mean
"the entry is in the index but ranked too low" -- never "the entry is missing".
(The Anki investigation showed 268/691 apparent failures were really absent
entries; that ambiguity is designed out here.)

`expected_jyutping` is deliberately NOT emitted.  `run_eval._match_result`
accepts a hit on jyutping alone, which lets a homophone of the intended word
score as a pass -- exactly the failure mode under test.

Hardness is not achieved by cherry-picking currently-failing queries, which
would overfit the set to today's ranker.  Instead every case carries a
`competitors` count (distinct other headwords sharing the reading) so the set
can be sliced by difficulty after the fact, and stays valid as the ranker moves.

Usage
-----
    python eval/build_spoken_corpus_eval.py
    python eval/build_spoken_corpus_eval.py --min-count 5 --out ...
"""

import argparse
import collections
import json
import os
import re
import sys

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
FULL = os.path.join(REPO, "full")
DEFAULT_OUT = os.path.join(REPO, "eval", "query_sets", "spoken_corpus.json")

CANTO_SOURCES = ("cccanto-webdist.txt", "cccedict-canto-readings-150923.txt")
CEDICT_LINE = re.compile(r"^(\S+)\s+(\S+)\s+\[([^\]]*)\]\s+\{([^}]*)\}")
GLOSS = re.compile(r"/(.*)/\s*$")
SYLLABLE = re.compile(r"[a-z]+[1-6]")
STOPWORDS = frozenset("a an the to of in on at is are be by or and for with as it "
                      "one that this from used use etc esp especially cl".split())

START_ID = 7001

# Jaccard overlap of English glosses above which two same-reading headwords are
# treated as spellings of one word rather than genuinely competing entries.
VARIANT_SIMILARITY = 0.3


def is_han(text):
    return len(text) > 0 and all("\u3400" <= c <= "\u9fff" for c in text)


def load_readings():
    """headword -> {jyutping reading}, from the project's own source files."""
    readings = collections.defaultdict(set)
    for name in CANTO_SOURCES:
        path = os.path.join(FULL, name)
        with open(path, encoding="utf-8") as handle:
            for line in handle:
                if line.startswith("#"):
                    continue
                match = CEDICT_LINE.match(line)
                if not match:
                    continue
                headword, _simplified, _pinyin, jyutping = match.groups()
                jyutping = " ".join(jyutping.lower().split())
                if jyutping:
                    readings[headword].add(jyutping)
    return readings


def load_glosses():
    """headword -> set of content words drawn from its English definitions.

    Used to spot orthographic variant pairs (部份/部分, 說話/説話), where the
    "wrong" answer means the same thing and demoting it is not really an error.
    """
    glosses = collections.defaultdict(set)
    for name in ("cccanto-webdist.txt", "cedict_ts.u8"):
        path = os.path.join(FULL, name)
        with open(path, encoding="utf-8") as handle:
            for line in handle:
                if line.startswith("#"):
                    continue
                match = GLOSS.search(line)
                if not match:
                    continue
                headword = line.split(" ", 1)[0]
                words = re.findall(r"[a-z]+", match.group(1).lower())
                glosses[headword] |= {w for w in words
                                      if len(w) > 2 and w not in STOPWORDS}
    return glosses


def gloss_similarity(glosses, left, right):
    a, b = glosses.get(left), glosses.get(right)
    if not a or not b:
        return 0.0
    return len(a & b) / len(a | b)


def split_syllables(jyutping):
    """HKCanCor stores readings unspaced ('zik1hai6'); the search box wants
    them spaced.  Returns None if the string is not cleanly parseable."""
    jyutping = jyutping.lower().strip()
    syllables = SYLLABLE.findall(jyutping)
    if not syllables or "".join(syllables) != jyutping:
        return None
    return " ".join(syllables)


def load_corpus_words(min_count):
    """(headword, spaced reading) -> corpus frequency, for multi-character words."""
    try:
        import pycantonese
    except ImportError:
        sys.exit("pycantonese is required: pip install pycantonese")

    counts = collections.Counter()
    for token in pycantonese.hkcancor().tokens():
        word, jyutping = token.word, token.jyutping
        if not jyutping or len(word) < 2 or not is_han(word):
            continue
        spaced = split_syllables(jyutping)
        if spaced is None or len(spaced.split()) != len(word):
            continue
        counts[(word, spaced)] += 1
    return {key: n for key, n in counts.items() if n >= min_count}


def build_competitor_index(readings):
    """reading -> {headwords with that reading}, for hardness scoring."""
    index = collections.defaultdict(set)
    for headword, jyutpings in readings.items():
        if not is_han(headword):
            continue
        for jyutping in jyutpings:
            index[jyutping].add(headword)
    return index


def band(count):
    if count >= 50:
        return "freq_very_high"
    if count >= 20:
        return "freq_high"
    if count >= 10:
        return "freq_mid"
    return "freq_low"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--min-count", type=int, default=3,
                        help="minimum HKCanCor occurrences (default: 3)")
    parser.add_argument("--out", default=DEFAULT_OUT)
    args = parser.parse_args()

    readings = load_readings()
    glosses = load_glosses()
    competitors = build_competitor_index(readings)
    corpus = load_corpus_words(args.min_count)

    dropped_missing = 0
    dropped_reading = 0
    cases = []

    for (word, spaced), count in sorted(corpus.items(), key=lambda kv: (-kv[1], kv[0])):
        known = readings.get(word)
        if not known:
            dropped_missing += 1
            continue
        if spaced not in known:
            # The corpus pronunciation is not one the dictionary records, so an
            # exact match is impossible by construction.  That is the 變調 case
            # and belongs to pin_jam.json, not here.
            dropped_reading += 1
            continue

        rival_words = competitors.get(spaced, set()) - {word}
        rivals = len(rival_words)
        synonymous = any(gloss_similarity(glosses, word, r) >= VARIANT_SIMILARITY
                         for r in rival_words)

        tags = ["spoken_corpus", band(count), "syl_%d" % len(word)]
        if rivals > 0:
            tags.append("has_competitors")
        if rivals >= 3:
            tags.append("many_competitors")
        if synonymous:
            # A rival means the same thing, so whichever ranks first is
            # arguably fine.  Excluded from the strict slice.
            tags.append("variant_risk")
        elif rivals > 0:
            tags.append("hard")

        cases.append({
            "id": START_ID + len(cases),
            "query": spaced,
            "category": "spoken_corpus",
            "description": "%s - said %d times in HKCanCor; %d other headword(s) share this reading"
                           % (word, count, rivals),
            "expected_characters": [word],
            "accept_top_n": 1,
            "tags": tags,
            "corpus_count": count,
            "competitors": rivals,
        })

    with open(args.out, "w", encoding="utf-8") as handle:
        json.dump(cases, handle, ensure_ascii=False, indent=2)
        handle.write("\n")

    print("HKCanCor words (>=%d occurrences): %d" % (args.min_count, len(corpus)))
    print("  dropped, headword not in dictionary : %d" % dropped_missing)
    print("  dropped, reading not in dictionary  : %d" % dropped_reading)
    print("  emitted                             : %d" % len(cases))
    tag_counts = collections.Counter(t for c in cases for t in c["tags"])
    for tag, n in sorted(tag_counts.items()):
        print("    %-16s %d" % (tag, n))
    print("wrote %s" % args.out)


if __name__ == "__main__":
    main()
