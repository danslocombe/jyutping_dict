# Plan — words.hk attested-word prior

## Question

Character frequency saturates (97.5% of characters sit at `MAX_STATIC_COST`) and
measures written/Mandarin-leaning usage, so it cannot tell an everyday Cantonese
word from an obscure Classical term. Does membership in a hand-curated Cantonese
dictionary supply the missing "a learner might search for this" signal?

## Hypothesis

Discounting entries attested in words.hk will promote everyday words above
corpus-frequent but irrelevant ones, improving p@1 without harming categories
that depend on length ordering.

## Method

1. Extract headwords from the words.hk CSV dump (`eval/build_wordshk_headwords.py`).
2. Mark matching entries at build time.
3. Apply a cost discount and sweep its size over the full eval suite.
4. Compare per-query positions against the discount=0 baseline to separate
   genuine improvements from churn.

## Success criteria

- Overall p@1 and MRR improve against baseline.
- `pin_jam` improves, since that set isolates the retrieved-but-demoted case.
- No category regresses materially, in particular `character` and
  `exact_vs_prefix`, which depend on shorter entries beating longer ones.

## Risks

- **Circularity.** Once words.hk feeds the ranker, any eval set derived from
  words.hk is invalid for measuring it.
- **Licence.** words.hk is not public domain. If the gain depends on the
  non-open subset, the result is not shippable as-is.
- **Length coupling.** Static cost carries both salience and length, so a naive
  discount may disturb length ordering. (This risk materialised — see findings.)
