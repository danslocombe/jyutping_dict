"""
Build an evaluation query set from hk150.json anki cards.

For each card, we extract the jyutping query and set expectations.
Cards with <= 3 syllables are expected at position 1.
Cards with 4 syllables get accept_top_n: 3.
Cards with 5+ syllables (full sentences) are skipped.
"""

import json
import re


def clean_jyutping(raw: str) -> str | None:
    """Extract a clean jyutping query from the anki card format."""
    jp = raw.strip()

    # Remove trailing punctuation
    jp = jp.rstrip("?!.")

    # Remove leading "..."
    jp = re.sub(r"^\.{3}\s*", "", jp)

    # Remove parenthetical annotations like (v.), (medical), (money), etc.
    jp = re.sub(r"\s*\([^)]*\)", "", jp)

    # Remove inline ellipsis (e.g., "ngo5 deoi3... man5 gam2")
    jp = jp.replace("...", " ")

    # If there are alternatives with /, take the first one
    if "/" in jp:
        jp = jp.split("/")[0].strip()

    # Replace hyphens with spaces
    jp = jp.replace("-", " ")

    # Collapse multiple spaces
    jp = re.sub(r"\s+", " ", jp).strip()

    # Skip if empty or doesn't look like jyutping
    if not jp:
        return None

    return jp


# Phrases that aren't single dictionary entries - these are multi-word
# constructions that we don't expect to find as a single result.
# We mark them with higher accept_top_n or skip entirely.
KNOWN_PHRASE_QUERIES = {
    # These are full phrases/constructions, not dictionary headwords
    "ngo5 m4 ming4",          # I don't understand (ngo5 + m4 ming4)
    "ngo5 zi1 dou3 la",       # I understand now!
    "ngo5 sik6 bou2 laa3",    # I am full
    "ngo5 hai6 jau4 haak3",   # I am a tourist
    "ngo5 jiu3 ni1 go3",      # I want this one
    "ngo5 sik6 zaai1 ge3",    # I am vegetarian
    "ngo5 deoi3 man5 gam2",   # I'm allergic to...
    "ni1 go3 jau5 mou5 juk6", # Does this have meat?
    "sik6 zou2 caan1",        # eat breakfast
    "sik6 ng5 caan1",         # eat lunch
    "sik6 maan5 caan1",       # eat dinner
    "hou2 hou2 mei6",         # Delicious!
    "dim2 gaai2 aa3",         # Why? (exclamation)
    "nei5 ne1",               # What about you?
    "ji4 gaa1 gei2 dim2",     # What time is it?
    "peng4 di1 dak1 m4 dak1", # Can you make it cheaper?
    "wifi mat6 maa5 hai6 me1", # What is the wifi password?
    "gei1 ceong4 faai3 sin3", # Airline Express
    "m4 jiu3 juk6",           # No meat please
    "san1 fu2 saai3",         # Thank you for the hard work
    "haa6 ci3 gin3",          # See you next time
    "gong2 maan6 di1",        # Talk a little slower
    "gong2 do1 ci3",          # Say it again
    "m4 hou2 ji3 si1",        # I'm sorry (slang)
    "m4 gan2 jiu3",           # That's fine
}


def count_syllables(query: str) -> int:
    """Count jyutping syllables (space-separated tokens)."""
    return len(query.split())


def map_category(anki_cat: str) -> str:
    """Map anki category to eval category."""
    mapping = {
        "Greetings & Politeness": "hk_greetings",
        "Core Essentials": "hk_core",
        "Numbers": "hk_numbers",
        "Food & Dining": "hk_food",
        "Transport": "hk_transport",
        "Money": "hk_money",
        "Directions & Places": "hk_directions",
        "Communication & Survival": "hk_communication",
        "Time Essentials": "hk_time",
        "Body & Medical": "hk_medical",
        "People & Small Talk": "hk_people",
        "Hotel": "hk_hotel",
    }
    return mapping.get(anki_cat, "hk_other")


def extract_definition_keywords(english: str) -> list[str]:
    """Extract clean definition keywords for matching.

    Strips parenthetical notes and takes the first alternative.
    Returns a list of keywords to match against definitions.
    """
    # Take text before first parenthetical
    base = re.split(r"\s*\(", english)[0].strip()
    # Take first alternative if / present
    base = base.split("/")[0].strip()
    # Remove leading articles and common noise
    base = re.sub(r"^(To |to |A |a |The |the )", "", base)
    # Strip trailing punctuation
    base = base.rstrip("!?.,;:")

    if not base or len(base) < 2:
        return []
    return [base]


def main():
    with open("D:/anki/hk150.json", "r", encoding="utf-8") as f:
        deck = json.load(f)

    test_cases = []
    skipped = []
    next_id = 5001  # Start after existing IDs

    for card in deck["cards"]:
        raw_jp = card["jyutping"]
        english = card["english"]
        category = card["category"]

        cleaned = clean_jyutping(raw_jp)
        if not cleaned:
            skipped.append({"english": english, "reason": "unparseable", "raw": raw_jp})
            continue

        n_syl = count_syllables(cleaned)

        # Skip full sentences (6+ syllables) - too long to expect ranking
        if n_syl >= 6:
            skipped.append({"english": english, "reason": f"{n_syl} syllables (sentence)", "raw": raw_jp, "cleaned": cleaned})
            continue

        # Skip known phrase constructions (not dictionary headwords)
        if cleaned in KNOWN_PHRASE_QUERIES:
            skipped.append({"english": english, "reason": "phrase (not headword)", "raw": raw_jp, "cleaned": cleaned})
            continue

        # Set accept_top_n based on length
        if n_syl <= 2:
            accept_top_n = 1
        elif n_syl == 3:
            accept_top_n = 2
        elif n_syl <= 5:
            accept_top_n = 3
        else:
            accept_top_n = 5

        # Build tags
        tags = ["hk_trip"]
        if n_syl == 1:
            tags.append("single_syllable")
        elif n_syl == 2:
            tags.append("two_syllable")
        else:
            tags.append("multi_syllable")

        # Use expected_jyutping with spaces (matches console output format)
        # For multi-syllable queries, jyutping alone is usually unique enough
        # For single-syllable, add definition_contains for disambiguation
        test_case = {
            "id": next_id,
            "query": cleaned,
            "category": map_category(category),
            "description": f"{english}",
            "expected_jyutping": [cleaned],
            "accept_top_n": accept_top_n,
            "tags": tags,
        }

        # Add definition matching for disambiguation
        def_kws = extract_definition_keywords(english)
        if def_kws:
            test_case["definition_contains"] = def_kws

        test_cases.append(test_case)
        next_id += 1

    # Write output
    output_path = "D:/ceot_maau/eval/query_sets/hk_trip.json"
    with open(output_path, "w", encoding="utf-8") as f:
        json.dump(test_cases, f, indent=2, ensure_ascii=False)

    # Summary
    print(f"Generated {len(test_cases)} test cases")
    print(f"Skipped {len(skipped)} cards:")
    for s in skipped:
        print(f"  - {s['english']}: {s['reason']}")

    # Breakdown by syllable count
    from collections import Counter
    syl_counts = Counter(count_syllables(tc["query"]) for tc in test_cases)
    print(f"\nSyllable distribution:")
    for n, count in sorted(syl_counts.items()):
        print(f"  {n} syllables: {count} cases")

    # Breakdown by category
    cat_counts = Counter(tc["category"] for tc in test_cases)
    print(f"\nCategory distribution:")
    for cat, count in sorted(cat_counts.items()):
        print(f"  {cat}: {count} cases")

    # Breakdown by accept_top_n
    top_n_counts = Counter(tc["accept_top_n"] for tc in test_cases)
    print(f"\nAccept top N distribution:")
    for n, count in sorted(top_n_counts.items()):
        print(f"  top {n}: {count} cases")


if __name__ == "__main__":
    main()
