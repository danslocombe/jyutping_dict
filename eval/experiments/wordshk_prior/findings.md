# words.hk attested-word prior

## Problem

Ranking is dominated by static cost, which is the sum of per-character frequency
costs. Two things go wrong:

1. `-1000 * ln(f)` clamped to `MAX_STATIC_COST = 7000` saturates for 9,689 of
   9,933 characters (97.5%), so only ~244 characters carry any frequency signal
   at all. 64% of 2-character entries have every character at the clamp.
2. What frequency remains measures *written corpus* frequency, which does not
   distinguish an everyday Cantonese word from a Classical or Mandarin-only term.

The result is that obscure entries outrank everyday ones:

| query | ranked #1 | expected | what #1 actually is |
|---|---|---|---|
| `bat1 sing1` | 畢昇 | 不勝 | an 11th-century printer |
| `tong1 fong2` | 惝恍 | 劏房 | literary Classical Chinese |
| `tou2 jau2` | 土丘 | 土魷 | "mound" |
| `gei2 cin2` | 幾千 | 幾錢 | "several thousand" |

Frequency data cannot fix this, because these words genuinely are frequent in
written Chinese. The missing signal is not frequency but *salience to a
Cantonese learner*.

## Hypothesis

words.hk is a hand-curated Cantonese dictionary. Mere membership is evidence
that a word is one a learner might search for. Of the project's 209,827 word
forms, 166,783 are absent from words.hk — that absence is the noise mass.

Classifying the 54 pin-jam demotions by membership:

```
33  both in words.hk          (genuine ambiguity / orthographic variants)
19  expected in, winner NOT   <-- addressable
 2  only winner in words.hk
```

## Attempt 1 — flat discount in builder.rs (failed)

Subtracting a flat discount from `entry.cost` at build time made things worse:
best MRR was the baseline, and single-character queries broke badly.

```
天  1 -> miss      火  1 -> miss      目  2 -> miss
```

**Why it failed:** static cost encodes salience *and* length in the same number
(~6,700 per extra character). Discounting salience therefore perturbs length
ordering, letting 2-character attested words leapfrog 1-character exact matches.
At discount 8,000 the full suite showed 17 improved against 29 regressed.

This is worth remembering: any change to static cost is also a change to the
length signal.

## Attempt 2 — flagged at build, applied at search (works)

Split the two concerns:

- `builder.rs` `mark_attested_words` sets a per-entry `attested` bit, leaving
  cost untouched. Serialised into the existing spare `flags: u8` (`FLAG_ATTESTED
  = 0x4`), so no index format change was needed.
- `search.rs` subtracts `WORDSHK_ATTESTED_BONUS` **only when the query accounts
  for the entry in full** — `unmatched_position_cost == 0` on the jyutping path,
  and `entry.characters.len() == query_terms.traditional_terms.len()` on the
  traditional path.

The guard is what makes it safe: an entry only receives the bonus when it is not
competing on length in the first place, so the length ordering cannot be
disturbed.

## Results

Full suite, 633 cases (includes the new `pin_jam` set):

| bonus | p@1 | p@3 | MRR | miss | character | pin_jam | exact_vs_prefix | hk_food |
|---|---|---|---|---|---|---|---|---|
| 0 (baseline) | 91% | 97% | 0.94 | 24 | 96% | 82% | 93% | 92% |
| 2,000 | 92% | 97% | 0.95 | 23 | 100% | 84% | 93% | 97% |
| 4,000 | 92% | 98% | 0.95 | 22 | 100% | 84% | 93% | 97% |
| **8,000** | **92%** | **98%** | **0.95** | **22** | **100%** | **85%** | **96%** | **97%** |
| 12,000 | 91% | 97% | 0.94 | 23 | 100% | 83% | 96% | 97% |
| 20,000 | 92% | 97% | 0.95 | 25 | 100% | 86% | 94% | 97% |

`WORDSHK_ATTESTED_BONUS = 8_000` is the chosen value: 21 queries improved
against 5 regressed, and it recovers two complete misses.

```
ge do          miss -> 1     lo ge          miss -> 1
目                2 -> 1     明天               2 -> 1
siu1 maai2       2 -> 1     siu1 ngo4         2 -> 1
tong1 fong2      2 -> 1     tou2 jau2         2 -> 1
sin4 jyu1        2 -> 1     loeng5 fan2       2 -> 1
```

Regressions are 2 English queries (`hot`, `sun`), 2 pin-jam and 1 transport
case. `hot` and `sun` parse as jyutping, so a promoted jyutping match displaces
the intended English one — unrelated to attestation and worth a separate look.

4,000 is the conservative alternative: 18 improved against only 2 regressed,
with identical headline metrics but 1pp less on `pin_jam`.

## Data and licence

`eval/build_wordshk_headwords.py` produces `full/wordshk-headwords.txt` from the
words.hk CSV dump.

**words.hk is not public domain.** It is © Hong Kong Lexicography Limited under
the Non-Commercial Open Data License (https://words.hk/base/hoifong/), and only
entries marked 已公開 are covered — 16,354 of 59,257.

The measurements above use **all** 53,391 headwords, because the open subset
covers only 5 of the 19 addressable demotions and shows no improvement over
baseline. Shipping this therefore needs one of:

- a licence conversation with words.hk (`info at words.hk`), or
- restricting to the 已公開 subset and accepting a much smaller gain, or
- reproducing the salience signal from a differently-licensed source.

The source CSVs and the generated headword list are gitignored. Attribution
requirements (copyright notice, disclaimer, credits list, link back) apply to any
distribution.

## Follow-ups

- The bonus is not applied on the English match path. `sorry` improved and
  `hot`/`sun` regressed purely through jyutping-path interactions; the English
  path deserves its own treatment.
- The underlying static-cost saturation is untouched. This prior compensates for
  it rather than fixing it; rescaling `-1000 * ln(f)` so fewer characters hit the
  clamp is the more fundamental change.
- 33 of the 54 pin-jam demotions have both candidates in words.hk, so
  attestation cannot separate them. Roughly half are orthographic variants where
  the "wrong" answer is arguably fine (干炒牛河/乾炒牛河, 冷親/冷嚫).
