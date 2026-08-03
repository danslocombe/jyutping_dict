# Ranking relevance — working notes

Tracking doc for the search relevance workstream. The reported symptom was that
terms which exist in the dictionary rank too low, particularly for multi-syllable
and composite terms.

## Diagnosis

### Static cost saturates

`builder.rs` computes a per-character cost as `-1000 * ln(frequency)` clamped to
`MAX_STATIC_COST = 7000`. The clamp bites at `f >= e^-7 ~= 0.0912%`, so **9,689 of
9,933 characters (97.5%) sit at the cap** and carry no frequency signal at all.
Only ~244 characters are differentiated.

Entry cost is the *sum* of per-character costs, so:

- 64% of 2-character and 45% of 3-character entries have every character at the cap.
- Mean static cost by length is 14,025 / 20,630 / 27,313 / 33,730 (~+6,700 per
  character), against a within-length standard deviation of only ~1,100-1,600.
- **Length outweighs frequency by roughly 6x.** Ranking within a length class is
  close to arbitrary.
- 155,952 entries collapse onto 13,127 distinct static costs. 44,232 entries share
  exactly 21,000; 20,631 share 14,000.
- In stored eval results, the median `static_cost / total_cost` for the rank-1 hit
  is **1.00**, and **211 of 983 queries had #1 and #2 tied on total cost**.

`search.rs` then adds a *second* length penalty via `unmatched_position_cost`
(`UNMATCHED_JYUTPING_PENALTY = 10_000` per unmatched position).

### Frequency is the wrong signal anyway

The frequency data reflects written, Mandarin-leaning corpora. It cannot tell an
everyday Cantonese word from a Classical or Mandarin-only term:

| query | ranked #1 | expected | what #1 is |
|---|---|---|---|
| `bat1 sing1` | 畢昇 | 不勝 | an 11th-century printer |
| `tong1 fong2` | 惝恍 | 劏房 | literary Classical Chinese |
| `tou2 jau2` | 土丘 | 土魷 | "mound" |
| `gei2 cin2` | 幾千 | 幾錢 | "several thousand" |

### The eval suite was overstating quality

Blind spots found in `run_eval.py` and the query sets (**all still unfixed**):

- `_match_result` accepts a hit if `expected_jyutping` matches, ignoring
  characters entirely. **148 of 983 queries scored as p@1 return a character that
  is not in `expected_characters`** — e.g. `gong2` returns 港 not 講, `money`
  returns 角, `hello` returns 喂.
- `hk_trip.json` (225 cases, 23% of the suite) has **no `expected_characters`
  field at all**, which also produces false negatives.
- Test IDs 800-816 are **duplicated** between `shorter_entry.json` and
  `tone_fuzzy.json`. `load_baseline` keys by id, so 13 cases are silently dropped
  and baseline diffs compare mismatched cases.
- Composite coverage was thin: 478 of 650 base queries were single-token, only 5
  expected a 3-character term, and none expected 4+.

An independent check against a personal Anki deck (691 terms) measured **72% p@1
tone-insensitive**, against the suite's reported 94%. By syllable count:
98% / 90% / 50% / 35% / 1% for 1 / 2 / 3 / 4 / 5+ syllables.

Importantly, that check also **corrected an earlier assumption**: of the terms that
failed, only 1 was a true ranking failure. 268 were simply absent from the source
dictionaries. Multi-syllable failure is mostly a data coverage problem, not a
ranking problem — so ranking work should be measured on terms known to exist.

## Work done

### 1. pin_jam query set (`eval/build_pin_jam_eval.py`)

300 cases (of 897 attested pairs) covering 變調, the colloquial tone change where
a word's final syllable is raised in speech but dictionaries record the formal
reading. A learner types what they hear, so 英文 must be findable as `jing1 man2`
even though CC-Canto stores `jing1 man4`.

Generated with no hand-labelling by exploiting a systematic disagreement between
two independent datasets: rime-cantonese's `word.csv` records actual spoken
pronunciation including sandhi, while CC-Canto records the formal citation
reading. Where characters and syllable bases match but only the final syllable's
tone differs, and it rises to tone 1 or 2, the pair is an attested tone change.
Six of the generated pairs were independently confirmed by the Anki deck.

Engine measurement: **p@1 82%, p@3 99%, not_found 0%**. The correct entry is
always retrieved, just demoted — exactly the reported symptom. Of the demotions,
~52% were caused by `JYUTPING_TONE_MISMATCH_PENALTY` (16,000) and ~48% by static
cost.

`expected_jyutping` is deliberately omitted from these cases: given the lenient
`_match_result` above, matching on the formal reading would let any homophone
satisfy the test.

### 2. words.hk attested-word prior

See `eval/experiments/wordshk_prior/findings.md` for the full write-up.

Entries whose traditional form appears in words.hk are flagged at build time
(`FLAG_ATTESTED`, packed into the existing spare `flags: u8`, so no index format
change). `search.rs` discounts them by `WORDSHK_ATTESTED_BONUS = 8_000`, **but
only when the query accounts for the entry in full**.

That guard is the whole trick. A first attempt applied the discount to
`entry.cost` in `builder.rs` and was net-negative (17 improved, 29 regressed),
breaking single-character queries — `天`, `火` and `目` all fell from rank 1 to
unranked. Static cost encodes salience *and* length in one number, so discounting
salience corrupts length ordering and lets longer attested words leapfrog shorter
exact matches.

Result on the full 633-case suite: p@1 91% -> 92%, p@3 97% -> 98%, MRR 0.94 ->
0.95, misses 24 -> 22. 21 queries improved, 5 regressed, two complete misses
recovered. Per category: character 96% -> 100%, hk_food 92% -> 97%,
exact_vs_prefix 93% -> 96%, tone_fuzzy 88% -> 100%, pin_jam 82% -> 85%.

**This adds no dictionary content.** Coverage is unchanged; it is one bit per
existing entry.

**Licence blocker.** words.hk is (c) Hong Kong Lexicography Limited under the
Non-Commercial Open Data License, not public domain, and only 16,354 of 59,257
entries are marked open. The gain above requires all 53,391 headwords — with the
open subset only, the sweep shows **no improvement over baseline**. Source CSVs
and the derived headword list are gitignored. This cannot ship until the licence
question is resolved.

## Open items

- [ ] Resolve the words.hk licence question, or replace the prior with an
      independently-licensed spoken-Cantonese frequency signal. The latter is
      preferable: a graded frequency would beat a boolean, and would attack the
      saturation problem directly rather than compensating for it.
      **Candidate found:** the LIHKG frequency list (MIT, 139,621 word forms,
      665M tokens) — see "Signals evaluated" below.
- [x] Build a larger, harder query set for validation, **not derived from
      words.hk** — see `spoken_corpus.json` below.
- [ ] Fix syllable-order insensitivity (see "Order insensitivity" below).
- [ ] Add a strict p@1 metric requiring a character match, to surface the 148
      hidden failures.
- [ ] Backfill `expected_characters` in `hk_trip.json`.
- [ ] Renumber the duplicate 800-816 IDs.
- [ ] Port the coverage-vs-ranking split (does the entry exist at all?) into
      `run_eval.py`, so data gaps stop being reported as ranking failures.
- [ ] Rescale the frequency model so fewer characters hit `MAX_STATIC_COST`. This
      is the root cause; everything above compensates for it.
- [ ] Add a relevance floor. With no cost cutoff, sentence-like queries return
      absurd results (`keoi5 hai6 ngo5 aa3 maa1` returns
      中華人民共和國香港特別行政區 at cost 1,146,805).
- [ ] Investigate the English match path. `hot` and `sun` regressed under the
      attested prior because they also parse as jyutping, so a promoted jyutping
      match displaces the intended English one.

## Gotchas

- `run_eval.CONSOLE_EXE` points at the **debug** build and `run_batch_queries`
  has `timeout=300`. Large query sets silently time out and report 0% accuracy.
  The tell-tale is that "Start batch..." prints but "Done running!" does not.
  Override to `console/target/release/console.exe`.
- The console reads relative data paths, so run it from the `console/` directory.
- Any change to static cost is also a change to the length signal.
- **Never validate a ranking signal against a query set derived from that same
  signal.** This is why the words.hk prior could not be validated on words.hk
  data, and why `spoken_corpus.json` is built from HKCanCor rather than from
  LIHKG — LIHKG is the next candidate ranking signal, so it must stay out of the
  eval sets or the same circularity returns one step removed.

---

# The `spoken_corpus` query set

`eval/build_spoken_corpus_eval.py` -> `eval/query_sets/spoken_corpus.json`
(1,346 cases, ids 7001+).

## Why HKCanCor

Every other query set is either hand-authored or derived from data that now
feeds the ranker. HKCanCor is independent of every ranking signal in the project:
30 hours of spontaneous Cantonese conversation recorded 1997-98, hand
transcribed, segmented and romanised. **CC BY 4.0**, obtained via `pycantonese`
(`pip install pycantonese`) — no vendored data, so nothing new to license.

It is also a better model of the actual complaint than a homophone-pair set:
every case is a word a real person actually said, looked up by the jyutping they
actually said it with.

## Construction

Every case is guaranteed to be a **ranking** test, never a coverage test — the
headword exists in the project's own Cantonese sources *and* the queried reading
is one those sources record for it. Of 1,641 candidate words, 189 were dropped as
absent from the dictionary and 106 because the corpus pronunciation was not a
recorded reading (that is the 變調 case, and belongs in `pin_jam.json`). So a
failure here can only mean "ranked too low", never "missing". This matters: in
the Anki investigation 268 of 691 apparent failures turned out to be absent
entries, and that ambiguity is designed out here.

`expected_jyutping` is deliberately omitted. `run_eval._match_result` accepts a
hit on jyutping alone, which lets a homophone of the intended word score as a
pass — precisely the failure mode under test.

Difficulty is **not** obtained by cherry-picking currently-failing queries, which
would overfit the set to today's ranker. Each case instead carries a
`competitors` count and tags, so the set can be sliced by difficulty after the
fact and stays valid as the ranker changes.

## Baseline (current ranker, words.hk prior enabled)

| Slice | n | p@1 | p@3 |
|---|---|---|---|
| all | 1346 | 96.9% | 99.9% |
| **0 competitors** | 1143 | **99.6%** | 99.9% |
| **1-2 competitors** | 194 | **83.0%** | 100% |
| **>=3 competitors** | 9 | **55.6%** | 100% |
| `hard` (competitors, variants excluded) | 166 | 84.9% | 100% |
| `variant_risk` | 37 | 67.6% | 100% |

Frequency band and syllable count barely move the number (96-98% across all
bands). **Competition is the entire story**, and it reproduces the reported
complaint as a clean gradient: a word is found reliably until something else
shares its reading, and then it is not.

As with `pin_jam`, p@3 is ~100% everywhere. The entry is always retrieved and
merely demoted, so this is a scoring problem, not a retrieval problem.

## The `variant_risk` tag

Roughly a fifth of the failures were pairs like 部份/部分, 說話/説話, 痴線/黐線 —
alternative spellings of one word, where whichever ranks first is arguably fine.
Counting these as errors would flatter or punish a change for no real reason, so
same-reading rivals whose English glosses overlap (Jaccard >= 0.3) are tagged
`variant_risk` and excluded from the `hard` slice. They fail far more often than
average (67.6% vs 96.9%), which is itself evidence the tag is picking out a real
category. **Use the `hard` slice (166 cases, 84.9%) as the headline number.**

## Order insensitivity (new bug)

The set surfaced a distinct defect. Querying `gei2 baak3` returns 百幾, 幾十百 and
others, but **幾百 — an exact, complete match for that reading — is not in the top
20 at all**, despite `幾百 ... {gei2 baak3}` being present in
`cccanto-webdist.txt`. Same shape for `jat1 maan6` (returns 萬一 over 一萬) and
`zau6 gam2` (returns 噉就 over 就噉).

Syllables appear to be matched as a set rather than a sequence, with no penalty
for reordering, and something then suppresses the in-order match entirely. This
is independent of the frequency/saturation issue and is likely a cheap, high-
value fix.

---

# Signals evaluated

## LIHKG frequency list — recommended next step

`https://raw.githubusercontent.com/AlienKevin/cantonese_frequency_list/56ec4da0963ad1842e755eb1e430df708803c0e2/freq.tsv`

**MIT licensed**, 139,621 word forms over 665,680,302 tokens, TSV `word\tcount`,
no header. No jyutping — join via rime-cantonese `word.csv`/`char.csv`.

Validated against 15 known failure pairs: **12 separate correctly**, 0 have both
sides absent. 劏房 11,745 vs 惝恍 **0**; 不勝 1,513 vs 畢昇 2; 夾錢 4,438 vs 合錢 5.
The 3 that go the "wrong" way are 米舖/米鋪 and 大棚/大棒 (orthographic variants,
either defensible) and 單于/善於 (LIHKG is arguably right).

The decisive measurement: frequency spans 10,836,313 down to 1. At the current
`MAX_STATIC_COST` of 7,000, **99.9% of words still saturate**; at 20,000 only
10.9% do; at 50,000, none. **The saturation is an artifact of the clamp value,
not of the data** — so this signal can attack the root cause rather than
compensating for it, and being MIT it also dissolves the words.hk licence
blocker.

Use `freq.tsv`, **not** `wordhk_freq.tsv` — the latter is filtered by words.hk
vocabulary and would reintroduce both the circularity and the licence problem.

## Other sources checked

- **Wiktionary via kaikki.org** (CC BY-SA 4.0) — only 3,864 Cantonese rows, most
  bare hanzi with no gloss. Far smaller than expected; weak as an oracle.
- **Cifu** (`gwinterstein/Cifu`) — has a uniquely valuable spoken-vs-written
  frequency split, but is **GPL-3.0**. LIHKG covers the same need under MIT.
- **rime-cantonese `word.csv`** has a `pron_rank` column (預設/常用/罕見/棄用) —
  a free commonness signal the project does not currently use. `variant.csv`
  marks non-Cantonese-native characters.
- Confirmed dead: `words.hk/static/all.csv.gz`,
  `words.hk/static/datasets/corpus_word_frequency.csv`,
  `dumps.wikimedia.org/yuewiki/`, `github.com/kfcd/cantondict`,
  `github.com/mahavivo/cantonese-wordlist`.
