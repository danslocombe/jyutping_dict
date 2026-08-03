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

/// Largest whole-word frequency discount band. The band lives in the 5 spare
/// bits of the compiled entry's `flags`, so it must fit in 31.
pub const MAX_FREQUENCY_DISCOUNT_BAND : u8 = 31;

/// Cost granularity of one frequency discount band.
pub const FREQUENCY_DISCOUNT_STEP : u32 = 800;

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
            let frequency_cost = cost;
            cost += cost_heuristic(&definitions.inner);
            cost = cost.saturating_sub(CCANTO_DISCOUNT);

            self.entries.push(DictionaryEntry {
                traditional: traditional.to_owned(),
                jyutping: jyutping.to_owned(),
                english_sets: definitions,
                source: EntrySource::CCanto,
                cost,
                attested: false,
                frequency_cost,
                frequency_discount_band: 0,
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

            let frequency_cost = cost;
            cost += cost_heuristic(&definitions.inner);

            //println!("{} - {:?}", traditional, definitions);
            self.entries.push(DictionaryEntry {
                traditional: traditional.to_owned(),
                jyutping: String::default(),
                english_sets: definitions,
                source: EntrySource::CEDict,
                cost,
                attested: false,
                frequency_cost,
                frequency_discount_band: 0 });
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

    /// Record how much cheaper a whole-word corpus frequency makes each entry
    /// than the sum of its character frequencies.
    ///
    /// Summing character costs says nothing about how common the *word* is, so
    /// 劏房 (11,745 occurrences) and 惝恍 (zero) score alike. Both costs come from
    /// the same `-1000 * ln(frequency)` curve, so their difference is meaningful;
    /// the absolute word cost is not, because a character sum also carries the
    /// length signal and a whole-word cost has no length term at all.
    ///
    /// Only reductions are recorded. A word attested even once would otherwise
    /// cost more than a word absent from the corpus entirely, since the fallback
    /// character sum is bounded by `MAX_STATIC_COST` per character while a hapax
    /// word is not. Keeping only the reduction makes the corpus a source of
    /// evidence *for* salience and never against it.
    ///
    /// Like `mark_attested_words` this deliberately stays out of `cost`. Applying
    /// it at build time reaches every match path, including the English one,
    /// where matching is definition *substring* containment with no notion of
    /// consuming the entry: discounting common compounds there let 西瓜 beat 水
    /// for "water" and 工作 fall five places for "work". The search applies the
    /// discount only where the query accounts for the entry in full, so all
    /// competing entries have the same character count and the discount can
    /// express salience alone.
    ///
    /// The list is optional, so a build without it behaves exactly as before.
    pub fn mark_word_frequencies(&mut self, frequencies : &WordFrequencies)
    {
        if (frequencies.inner.is_empty()) {
            return;
        }

        let total = frequencies.total();
        let mut marked = 0;
        for e in &mut self.entries
        {
            if let Some(count) = frequencies.inner.get(&e.traditional) {
                let word_cost = WordFrequencies::cost(*count, total);
                let discount = e.frequency_cost.saturating_sub(word_cost);
                let band = (discount / FREQUENCY_DISCOUNT_STEP).min(MAX_FREQUENCY_DISCOUNT_BAND as u32) as u8;

                if (band > 0) {
                    e.frequency_discount_band = band;
                    marked += 1;
                }
            }
        }

        println!("Marked {} entries with a whole-word frequency discount", marked);
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
    /// The part of `cost` derived from character frequency, kept separately so
    /// that it can be compared against a whole-word frequency.
    pub frequency_cost: u32,
    /// How much cheaper the whole-word corpus frequency makes this entry than
    /// its character sum, in units of `FREQUENCY_DISCOUNT_STEP`. 0 when unknown.
    /// See `Builder::mark_word_frequencies`.
    pub frequency_discount_band: u8,
}

/// Whole-word corpus counts, keyed by traditional headword.
///
/// The static cost model sums *character* frequencies, so it has no notion of
/// how common a word is. That is the root of the reported ranking complaint:
/// 劏房 and 惝恍 are built from comparably rare characters and score alike, even
/// though one is everyday Hong Kong vocabulary and the other appears zero times
/// in 665 million tokens of Cantonese forum text.
///
/// Format is one `word\tcount` per line, no header.
#[derive(Debug, Default)]
pub struct WordFrequencies
{
    pub inner: std::collections::HashMap<String, u64>,
}

impl WordFrequencies
{
    pub fn parse(path : &str) -> Self
    {
        let data = match std::fs::read_to_string(path) {
            Ok(d) => d,
            Err(_) => {
                println!("No word frequency list at {}, skipping", path);
                return Self::default();
            }
        };

        let mut inner = std::collections::HashMap::new();
        for line in data.lines()
        {
            if (line.is_empty() || line.starts_with('#')) {
                continue;
            }

            if let Some((word, count_str)) = line.split_once('\t') {
                if let Ok(count) = count_str.trim().parse::<u64>() {
                    if (!word.is_empty() && count > 0) {
                        inner.insert(word.to_owned(), count);
                    }
                }
            }
        }

        println!("Read {} word frequencies", inner.len());

        Self { inner }
    }

    /// Map a raw count onto a static cost.
    ///
    /// Deliberately the same `-1000 * ln(frequency)` curve the character model
    /// uses, so word-derived and character-derived costs live on one scale and
    /// can be compared directly.
    pub fn cost(count : u64, total : u64) -> u32
    {
        let frequency = (count as f64) / (total as f64);
        let cost = -1_000.0 * frequency.ln();
        cost.clamp(1.0, u32::MAX as f64) as u32
    }

    pub fn total(&self) -> u64
    {
        self.inner.values().sum()
    }
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

#[cfg(test)]
mod tests
{
    use super::*;

    fn entry(traditional : &str, frequency_cost : u32) -> DictionaryEntry
    {
        DictionaryEntry {
            traditional: traditional.to_owned(),
            jyutping: String::default(),
            english_sets: StringVecSet::default(),
            source: EntrySource::CEDict,
            cost: frequency_cost,
            attested: false,
            frequency_cost,
            frequency_discount_band: 0,
        }
    }

    /// `WordFrequencies::total` is the sum of the map, so a fixture needs enough
    /// padding for the probabilities to be realistic.
    fn frequencies(pairs : &[(&str, u64)], total : u64) -> WordFrequencies
    {
        let mut inner : std::collections::HashMap<String, u64> =
            pairs.iter().map(|(w, c)| ((*w).to_owned(), *c)).collect();

        inner.insert("\u{0}padding".to_owned(), total - inner.values().sum::<u64>());

        WordFrequencies { inner }
    }

    #[test]
    fn test_word_cost_matches_the_character_cost_curve()
    {
        // Both models are -1000 * ln(frequency), so a word ten times commoner
        // costs ln(10) * 1000 less. Sharing the curve is what makes a word cost
        // and a character cost comparable enough to subtract.
        let sparse = WordFrequencies::cost(100, 1_000_000);
        let common = WordFrequencies::cost(1_000, 1_000_000);

        assert_eq!(sparse - common, 2_303);
    }

    #[test]
    fn test_discount_records_how_much_cheaper_the_whole_word_is()
    {
        // 8,000 of character cost against a word cost of -1000*ln(1/1000) = 6,907.
        let mut builder = Builder::default();
        builder.entries.push(entry("劏房", 8_000));
        builder.mark_word_frequencies(&frequencies(&[("劏房", 1_000)], 1_000_000));

        let expected = (8_000 - 6_907) / FREQUENCY_DISCOUNT_STEP;
        assert_eq!(builder.entries[0].frequency_discount_band as u32, expected);
    }

    #[test]
    fn test_a_word_rarer_than_its_characters_is_not_penalised()
    {
        // The character sum is bounded by MAX_STATIC_COST per character but a
        // word cost is not, so almost every rare word looks "worse" than its
        // characters. Recording that would make the corpus evidence against
        // salience, so only reductions count.
        let mut builder = Builder::default();
        builder.entries.push(entry("惝恍", 3_000));
        builder.mark_word_frequencies(&frequencies(&[("惝恍", 1)], 1_000_000));

        assert_eq!(builder.entries[0].frequency_discount_band, 0);
    }

    #[test]
    fn test_words_absent_from_the_corpus_are_untouched()
    {
        let mut builder = Builder::default();
        builder.entries.push(entry("噚日", 12_000));
        builder.mark_word_frequencies(&frequencies(&[("尋日", 60_000)], 1_000_000));

        assert_eq!(builder.entries[0].frequency_discount_band, 0);
    }

    #[test]
    fn test_discount_band_is_clamped_to_the_five_bits_available()
    {
        // The band shares a u8 with the source and attested flags.
        let mut builder = Builder::default();
        builder.entries.push(entry("的", u32::MAX));
        builder.mark_word_frequencies(&frequencies(&[("的", 500_000)], 1_000_000));

        assert_eq!(builder.entries[0].frequency_discount_band, MAX_FREQUENCY_DISCOUNT_BAND);
    }

    #[test]
    fn test_marking_frequencies_leaves_cost_alone()
    {
        // The discount is applied at search time, where it can be gated on the
        // query accounting for the entry in full. Folding it into cost here
        // would also reach the English path, which matches definition
        // substrings and let 西瓜 outrank 水 for "water".
        let mut builder = Builder::default();
        builder.entries.push(entry("西瓜", 12_000));
        builder.mark_word_frequencies(&frequencies(&[("西瓜", 15_000)], 1_000_000));

        assert_eq!(builder.entries[0].cost, 12_000);
    }
}
