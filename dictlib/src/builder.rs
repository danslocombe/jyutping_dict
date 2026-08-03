use std::collections::BTreeMap;
use crate::{EntrySource, StringVecSet};

#[derive(Debug, Default)]
pub struct Builder
{
    pub trad_to_frequency : TraditionalToFrequencies,
    pub entries: Vec<DictionaryEntry>,
}

pub const MAX_STATIC_COST_F : f32 = 7_000.0;
pub const MAX_STATIC_COST   : u32 = 7_000;

/// Per-character cost override for high-frequency Cantonese-specific characters
/// that have no Mandarin equivalent and no entry in the frequency file.
pub const CANTO_HIGH_FREQ_COST: u32 = 1_000;

/// Flat cost discount applied to all CCanto-sourced entries.
pub const CCANTO_DISCOUNT: u32 = 0;

/// Tier 1: Extremely common Cantonese function words/particles/pronouns
const CANTO_TIER1_CHARS: &[char] = &[
    '嘅', // ge3 - possessive particle
    '佢', // keoi5 - he/she/it
    '係', // hai6 - to be
    '冇', // mou5 - don't have
    '喺', // hai2 - at/in
    '咁', // gam3 - so/such
    '嗰', // go2 - that
    '啲', // di1 - some/a bit
];

/// Tier 2: Common Cantonese verbs and content words
const CANTO_TIER2_CHARS: &[char] = &[
    '嘢', // je5 - thing/stuff
    '噉', // gam2 - like this
    '攞', // lo2 - to take
    '嚟', // lai4 - to come
    '揾', // wan2 - to find
];

impl Builder {
    pub fn parse_ccanto(&mut self, path : &str, trad_to_frequency : &TraditionalToFrequencies)
    {
        let size_at_start = self.entries.len();

        let data = std::fs::read_to_string(path).unwrap();
        for line in data.lines()
        {
            if (line.len() == 0) {
                continue;
            }
            if (line.starts_with('#')) {
                continue;
            }

            // Expect form
            // Traditional Simplified [pinyin] {jyutping} /Definition0/Definition1/../

            let (traditional, rest) = line.split_once(' ').unwrap();
            let (_simplified, rest) = rest.split_once(' ').unwrap();

            assert!(rest.len() > 0);
            assert_eq!(rest.chars().next().unwrap(), '[');

            let pinyin_end = rest.find(']').unwrap();

            let rest = &rest[pinyin_end+2..];

            assert!(rest.len() > 0);
            assert_eq!(rest.chars().next().unwrap(), '{');
            let jyutping_end = rest.find('}').unwrap();
            let jyutping = &rest[1..jyutping_end];

            let mut english = &rest[jyutping_end+2..];

            if let Some(end_comment) = english.find('#')
            {
                english = &english[0..end_comment];
            }

            let mut definitions = StringVecSet::default();
            for def in english.split("/")
            {
                let def = def.trim();
                if (def.len() == 0) {
                    continue;
                }

                definitions.add_clone(def);
            }

            // Use frequency-based cost (same as CEDict) so CCanto entries
            // are directly comparable in ranking
            let mut cost = 0u32;
            for c in traditional.chars() {
                cost += trad_to_frequency.get_or_default(c).cost;
            }
            cost += cost_heuristic(&definitions.inner);
            cost = cost.saturating_sub(CCANTO_DISCOUNT);

            self.entries.push(DictionaryEntry {
                traditional: traditional.to_owned(),
                jyutping: jyutping.to_owned(),
                english_sets: definitions,
                source: EntrySource::CCanto,
                cost,
                attested: false,
            });
        }

        println!("Read {} dictionary entries from {}", {self.entries.len() - size_at_start}, path);
    }

    pub fn annotate(&mut self, trad_to_jyutping: &TraditionalToJyutping) {
        for e in &mut self.entries {
            if let Some(j) = trad_to_jyutping.inner.get(&e.traditional) {
                e.jyutping = j.inner[0].to_owned();
            }
        }
    }

    pub fn parse_cedict(&mut self, path : &str, trad_to_frequency : &TraditionalToFrequencies)
    {
        let size_at_start = self.entries.len();

        let data = std::fs::read_to_string(path).unwrap();
        for line in data.lines()
        {
            if (line.len() == 0) {
                continue;
            }
            if (line.starts_with('#')) {
                continue;
            }
            // Expect form
            // Traditional Simplified [pinyin] /Definition0/Definition1/../

            let (traditional, rest) = line.split_once(' ').unwrap();

            let (_simplified, rest) = rest.split_once(' ').unwrap();

            assert!(rest.len() > 0);
            assert_eq!(rest.chars().next().unwrap(), '[');

            let pinyin_end = rest.find(']').unwrap();

            let mut english = &rest[pinyin_end+2..];

            if let Some(end_comment) = english.find('#')
            {
                english = &english[0..end_comment];
            }

            let mut definitions = StringVecSet::default();
            for def in english.split("/")
            {
                let def = def.trim();
                if (def.len() == 0) {
                    continue;
                }

                definitions.add_clone(def);
            }

            let mut cost = 0;
            for c in traditional.chars() {
                cost += trad_to_frequency.get_or_default(c).cost;
            }

            cost += cost_heuristic(&definitions.inner);

            //println!("{} - {:?}", traditional, definitions);
            self.entries.push(DictionaryEntry {
                traditional: traditional.to_owned(),
                jyutping: String::default(),
                english_sets: definitions,
                source: EntrySource::CEDict,
                cost,
                attested: false });
        }

        println!("Read {} dictionary entries from {}", {self.entries.len() - size_at_start}, path);
    }

    pub fn apply_additional_heuristics(&mut self)
    {
        for e in &mut self.entries
        {
            // No jyutping, probably not a good entry
            if (e.jyutping.is_empty())
            {
                e.cost += 10_000;
            }
        }
    }

    /// Mark entries attested in words.hk, a hand-curated Cantonese dictionary.
    ///
    /// Character frequency cannot distinguish a word Cantonese speakers actually
    /// use from a Classical or Mandarin-only term that is frequent in written
    /// corpora, so obscure entries outrank everyday ones (畢昇 over 不勝, 惝恍
    /// over 劏房). Membership is recorded as a flag rather than folded into the
    /// cost here because static cost also carries the length signal; the search
    /// applies the bonus only where it cannot disturb length ordering.
    ///
    /// The file is one traditional headword per line and is optional, so a build
    /// without it behaves exactly as before.
    pub fn mark_attested_words(&mut self, path : &str)
    {
        let data = match std::fs::read_to_string(path) {
            Ok(d) => d,
            Err(_) => {
                println!("No attested word list at {}, skipping", path);
                return;
            }
        };

        let attested : std::collections::HashSet<&str> =
            data.lines().map(|l| l.trim()).filter(|l| !l.is_empty()).collect();

        let mut marked = 0;
        for e in &mut self.entries
        {
            if (attested.contains(e.traditional.as_str()))
            {
                e.attested = true;
                marked += 1;
            }
        }

        println!("Marked {} entries attested in {} ({} headwords)", marked, path, attested.len());
    }
}

enum Heuristic
{
    ContainsTerms(&'static [&'static str]),
    DoesNotContainTerms(&'static [&'static str]),
}

const HEURISTICS : &[(Heuristic, u32)] = &[
    (Heuristic::ContainsTerms(&["abbr."]), 5_000),
    (Heuristic::DoesNotContainTerms(&["M:", "CL:"]), 5_000),
    (Heuristic::ContainsTerms(&["Surname", "surname"]), 2_000),
    (Heuristic::DoesNotContainTerms(&["(Cantonese)"]), 2_000),
    (Heuristic::ContainsTerms(&["Confucius"]), 5_000),
    (Heuristic::ContainsTerms(&["Dynasty", "Dynasties"]), 5_000),
    (Heuristic::ContainsTerms(&["(Buddhism)"]), 5_000),
];

fn cost_heuristic(english_definitions: &[String]) -> u32
{
    //let from_number_of_defs: u32 = 1000 - english_definitions.len().min(10) as u32 * 100;

    let mut cost = 0;

    for (heuristic, c) in HEURISTICS {
        match heuristic {
            Heuristic::ContainsTerms(terms) => {
                if (matches_terms(terms, english_definitions)) {
                    cost += c;
                }
            },
            Heuristic::DoesNotContainTerms(terms) => {
                if (!matches_terms(terms, english_definitions)) {
                    cost += c;
                }
            }
        }
    }

    cost
}

fn matches_terms(needles: &[&str], heystacks: &[String]) -> bool {
    for needle in needles {
        for heystack in heystacks {
            if (heystack.contains(needle)) {
                return true;
            }
        }
    }

    false
}

#[derive(Debug)]
pub struct DictionaryEntry
{
    pub cost: u32,
    pub traditional: String,
    pub jyutping: String,
    pub english_sets: StringVecSet,
    pub source: EntrySource,
    /// Whether the word appears in a curated Cantonese dictionary (words.hk).
    pub attested: bool,
}

#[derive(Debug, Default)]
pub struct TraditionalToJyutping
{
    pub inner : BTreeMap<String, StringVecSet>,
    pub reverse : BTreeMap<String, StringVecSet>,
}

impl TraditionalToJyutping
{
    pub fn add(&mut self, chars : &str, jyutping: &str) {
        if let Some(x) = self.inner.get_mut(chars) {
            x.add_clone(jyutping);
        }
        else {
            self.inner.insert(chars.to_owned(), StringVecSet::single(jyutping.to_owned()));
        }

        if let Some(x) = self.reverse.get_mut(jyutping) {
            x.add_clone(chars);
        }
        else {
            self.reverse.insert(jyutping.to_owned(), StringVecSet::single(chars.to_owned()));
        }
    }

    pub fn parse(path : &str) -> Self
    {
        let mut map = Self::default();
        let data = std::fs::read_to_string(path).unwrap();
        for line in data.lines()
        {
            if (line.len() == 0) {
                continue;
            }
            if (line.starts_with('#')) {
                continue;
            }

            // Expect form
            // Traditional Simplified [pinyin] {jyutping}

            let (traditional, rest) = line.split_once(' ').unwrap();
            let (_simplified, rest) = rest.split_once(' ').unwrap();

            assert!(rest.len() > 0);
            assert_eq!(rest.chars().next().unwrap(), '[');
            let pinyin_end = rest.find(']').unwrap();

            let jyutping_with_brackets = &rest[pinyin_end+2..];
            assert!(jyutping_with_brackets.len() > 0);
            assert_eq!(jyutping_with_brackets.chars().next().unwrap(), '{');

            let jyutping = jyutping_with_brackets[1..jyutping_with_brackets.len() - 1].to_owned();
            //println!("{} - {}", traditional, jyutping);
            map.add(traditional, &jyutping);
        }

        println!("Read {} jyutping romanisations", {map.inner.len()});
        map
    }
}

#[derive(Debug, Clone, Copy)]
pub struct FrequencyData
{
    count : i32,
    frequency : f32,
    cost : u32,
    index : i32,
}

#[derive(Debug, Default)]
pub struct TraditionalToFrequencies
{
    inner : BTreeMap<char, FrequencyData>,
}

impl TraditionalToFrequencies
{
    pub fn get_frequencies(&self, characters : &str) -> Vec<FrequencyData> {
        let mut frequencies = Vec::new();

        for c in characters.chars() {
            frequencies.push(self.get_or_default(c));
        }

        frequencies
    }

    pub fn get_or_default(&self, character : char) -> FrequencyData {
        if let Some(x) = self.inner.get(&character) {
            *x
        }
        else {
            FrequencyData {
                index : self.inner.len() as i32 + 1,
                count: 0,
                frequency: 0.0,
                cost: MAX_STATIC_COST,
            }
        }
    }

    pub fn add_canto(&mut self, characters: &str) {
        for c in characters.chars() {
            // HACK
            //self.inner.entry(c).or_insert(FrequencyData { count: 1, frequency: 0.001, cost: 2.0, index: 10_000 });
            self.inner.entry(c).or_insert(FrequencyData { count: 1, frequency: 0.001, cost: 10_000, index: 10_000 });
        }
    }

    pub fn parse(path : &str) -> Self
    {
        let mut inner = BTreeMap::new();

        let data = std::fs::read_to_string(path).unwrap();
        let mut last_cumulative_frequency_percentile : f32 = 0.0;
        for line in data.lines()
        {
            if (line.len() == 0) {
                continue;
            }
            if (line.starts_with('#')) {
                continue;
            }

            // Expect form
            // index \t character \t count \t cumulative frequency percentile \t pinyin \t english

            let (index_str, rest) = line.split_once('\t').unwrap();
            let (character, rest) = rest.split_once('\t').unwrap();
            let (count_str, rest) = rest.split_once('\t').unwrap();
            let (cumulative_frequency_percentile_str, _rest) = rest.split_once('\t').unwrap();

            let index : i32 = index_str.parse().unwrap();
            let count : i32 = count_str.parse().unwrap();
            let cumulative_frequency_percentile : f32 = cumulative_frequency_percentile_str.parse().unwrap();

            let frequency = (cumulative_frequency_percentile - last_cumulative_frequency_percentile) / 100.0;
            last_cumulative_frequency_percentile = cumulative_frequency_percentile;

            let cost = -1_000.0 * frequency.ln();
            let cost = cost.clamp(1.0, MAX_STATIC_COST_F) as u32;

            let data = FrequencyData {
                count, frequency, index, cost,
            };

            inner.insert(character.chars().next().unwrap(), data);
        }

        println!("Read {} character frequencies", {inner.len()});

        Self {
            inner,
        }
    }

    /// For traditional characters missing from the frequency file, fall back to
    /// the simplified form's frequency data if available. The trad→simp mapping
    /// is extracted from the dictionary file (first two columns: Traditional Simplified).
    pub fn add_simplified_fallbacks(&mut self, dict_path: &str)
    {
        let data = std::fs::read_to_string(dict_path).unwrap();
        let mut fallback_count = 0;
        for line in data.lines()
        {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let (traditional, rest) = match line.split_once(' ') {
                Some(x) => x,
                None => continue,
            };
            let (simplified, _rest) = match rest.split_once(' ') {
                Some(x) => x,
                None => continue,
            };

            for (trad_char, simp_char) in traditional.chars().zip(simplified.chars())
            {
                if trad_char != simp_char
                    && !self.inner.contains_key(&trad_char)
                {
                    if let Some(simp_data) = self.inner.get(&simp_char).copied() {
                        self.inner.insert(trad_char, simp_data);
                        fallback_count += 1;
                    }
                }
            }
        }
        println!("Added {} simplified→traditional frequency fallbacks from {}", fallback_count, dict_path);
    }

    /// Insert cost overrides for Cantonese-specific characters that have no
    /// Mandarin equivalent and are missing from the frequency file.
    pub fn add_cantonese_overrides(&mut self)
    {
        let tier2_cost = CANTO_HIGH_FREQ_COST + 1_000;
        let mut count = 0;

        for &c in CANTO_TIER1_CHARS {
            self.inner.insert(c, FrequencyData {
                count: 1, frequency: 0.0, cost: CANTO_HIGH_FREQ_COST, index: 10_001,
            });
            count += 1;
        }

        for &c in CANTO_TIER2_CHARS {
            self.inner.insert(c, FrequencyData {
                count: 1, frequency: 0.0, cost: tier2_cost, index: 10_002,
            });
            count += 1;
        }

        println!("Added {} Cantonese character cost overrides (tier1={}, tier2={})", count, CANTO_HIGH_FREQ_COST, tier2_cost);
    }
}
