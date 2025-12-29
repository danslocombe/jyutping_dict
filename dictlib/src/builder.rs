use std::collections::BTreeMap;
use crate::{EntrySource, JyutpingSplitter, StringVecSet};

#[derive(Debug, Default)]
pub struct Builder
{
    pub trad_to_frequency : TraditionalToFrequencies,
    pub entries: Vec<DictionaryEntry>,
}

impl Builder {
    pub fn parse_ccanto(&mut self, path : &str)
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

            //trad_to_frequency.add_canto(&traditional);
            let mut jyutping_count = 0;
            for _ in JyutpingSplitter::new(jyutping) {
                jyutping_count += 1;
            }

            let mut cost = (15_000 + jyutping_count * 1_000) as u32;
            cost += cost_heuristic(&definitions.inner);

            self.entries.push(DictionaryEntry {
                traditional: traditional.to_owned(),
                jyutping: jyutping.to_owned(),
                english_sets: definitions,
                source: EntrySource::CCanto,
                cost,
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
                cost });
        }

        println!("Read {} dictionary entries from {}", {self.entries.len() - size_at_start}, path);
    }
}

enum Heuristic
{
    ContainsTerms(&'static [&'static str]),
    DoesNotContainTerms(&'static [&'static str]),
}

const HEURISTICS : &[(Heuristic, u32)] = &[
    (Heuristic::ContainsTerms(&["abbr."]), 5000),
    (Heuristic::DoesNotContainTerms(&["M:", "CL:"]), 5000),
    (Heuristic::ContainsTerms(&["Surname", "surname"]), 2000),
    (Heuristic::DoesNotContainTerms(&["(Cantonese)"]), 2000),
    (Heuristic::ContainsTerms(&["Confucius"]), 5000),
    (Heuristic::ContainsTerms(&["Dynasty", "Dynasties"]), 5000),
    (Heuristic::ContainsTerms(&["(Buddhism)"]), 5000),
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
                cost: 64_000,
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
            let cost = cost.clamp(1.0, 64_000.0) as u32;

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
}
