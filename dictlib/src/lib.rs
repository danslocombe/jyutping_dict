#![allow(dead_code)]
#![allow(unused_parens)]

use std::collections::BTreeMap;

#[macro_export]
macro_rules! debug_log {
    ( $( $t:tt )* ) => {
        $crate::debug_logline(&format!( $( $t )* ));
    }
}

#[macro_export]
macro_rules! error_log {
    ( $( $t:tt )* ) => {
        $crate::error_logline(&format!( $( $t )* ));
    }
}

pub mod compiled_dictionary;
pub mod jyutping_splitter;
pub mod data_writer;
pub mod data_reader;
pub mod vbyte;
pub mod string_search;

static mut DEBUG_LOGGER : Option<Box<dyn DebugLogger>> = None;

pub fn set_debug_logger(logger : Box<dyn DebugLogger>) {
    // Only should be called once by main thread at init
    unsafe {
        DEBUG_LOGGER = Some(logger);
    }
}

pub fn debug_logline(logline : &str)
{
    unsafe { if let Some(x) = DEBUG_LOGGER.as_ref() { x.log(logline); }}
}

pub fn error_logline(logline : &str)
{
    unsafe { if let Some(x) = DEBUG_LOGGER.as_ref() { x.log_error(logline); }}
}

pub trait DebugLogger {
    fn log(&self, logline: &str);
    fn log_error(&self, logline: &str);
}

#[derive(Debug, Clone, Copy)]
pub struct OffsetString {
    pub start: u32,
    pub len: u32,
}

#[derive(Debug)]
pub struct DictionaryEntry
{
    pub cost: u32,
    pub traditional: String,
    pub jyutping: String,
    pub english_sets: StringVecSet,
}

#[derive(Debug, Default)]
pub struct Dictionary
{
    //pub trad_to_def: TraditionalToDefinitions,
    //pub trad_to_jyutping : TraditionalToJyutping,
    pub trad_to_frequency : TraditionalToFrequencies,
    pub entries: Vec<DictionaryEntry>,
}


impl Dictionary {
    //pub fn hacky_search(&self, query : &str) -> Vec<SearchResult>{
    //    let queries : Vec<String> = query.split(' ').map(|x| x.trim().to_owned()).collect();

    //    println!("Queries {:?}", queries);

    //    let mut results = Vec::new();

    //    // TODO trie
    //    for (jyutping, v) in &self.trad_to_jyutping.reverse
    //    {
    //        let mut matches = true;
    //        for q in &queries {
    //            if (!jyutping.contains(q)) {
    //                matches = false;
    //            }
    //        }

    //        if (matches)
    //        {
    //            for characters in &v.inner {
    //                let frequency_data = self.trad_to_frequency.get_frequencies(characters);
    //                let definitions = self.trad_to_def.inner.get(characters).map(|x| x.clone()).unwrap_or_default();

    //                let res = SearchResult {
    //                    characters: characters.to_owned(),
    //                    jyutping: jyutping.to_owned(),
    //                    definitions: definitions.inner,
    //                    frequency_data: frequency_data.to_owned(),
    //                };

    //                results.push(res);
    //            }
    //        }
    //    }

    //    results.sort_by(|x, y| x.cost().cmp(&y.cost()));

    //    results.into_iter().take(10).collect()
    //}

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

            self.entries.push(DictionaryEntry {
                traditional: traditional.to_owned(),
                jyutping: jyutping.to_owned(),
                english_sets: definitions,
                cost: 10_000 * traditional.len() as u32 });
            //println!("{} - {:?}", traditional, definitions);
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
            
            //println!("{} - {:?}", traditional, definitions);
            self.entries.push(DictionaryEntry {
                traditional: traditional.to_owned(),
                jyutping: String::default(),
                english_sets: definitions,
                cost });
        }

        println!("Read {} dictionary entries from {}", {self.entries.len() - size_at_start}, path);
    }
}

#[derive(Debug)]
pub struct SearchResult {
    characters : String,
    jyutping : String,
    definitions : Vec<String>,
    frequency_data : Vec<FrequencyData>,
}

impl SearchResult {
    pub fn cost(&self) -> u32 {
        let mut sum = 0;
        for freq in &self.frequency_data {
            sum += freq.cost;
        }

        sum
    }
}

#[derive(Default, Debug)]
pub struct TraditionalToDefinitions
{
    inner : BTreeMap<String, StringVecSet>,
}

impl TraditionalToDefinitions
{
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
            if self.inner.get(&c).is_none() {
                // HACK
                //self.inner.insert(c, FrequencyData { count: 1, frequency: 0.001, cost: 2.0, index: 10_000 });
                self.inner.insert(c, FrequencyData { count: 1, frequency: 0.001, cost: 10_000, index: 10_000 });
            }
        }
    }

    pub fn parse(path : &str) -> Self
    {
        let mut inner = BTreeMap::new();

        let data = std::fs::read_to_string(path).unwrap();
        let mut last_cumulative_frequency_percentile : f64 = 0.0;
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
            let cumulative_frequency_percentile : f64 = cumulative_frequency_percentile_str.parse().unwrap();

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

#[derive(Debug, Clone, Copy)]
pub struct FrequencyData
{
    count : i32,
    frequency : f64,
    cost : u32,
    index : i32,
}

#[derive(Debug, Clone, Default)]
pub struct StringVecSet
{
    pub inner: Vec<String>,
}

impl StringVecSet {
    pub fn single(x: String) -> Self {
        Self {
            inner: vec![x],
        }
    }
    pub fn contains(&self, x: &str) -> bool {
        for xx in &self.inner {
            if (xx.eq_ignore_ascii_case(x)) {
                return true;
            }
        }

        return false;
    }

    pub fn add_clone(&mut self, val: &str) {
        if (!self.contains(val))
        {
            self.inner.push(val.to_owned());
        }
    }

    pub fn add(&mut self, val: String) {
        if (!self.contains(&val))
        {
            self.inner.push(val);
        }
    }

    // Similar to Vec::extend, drain other and add to our collection.
    pub fn extend(&mut self, other: StringVecSet) {
        for x in other.inner {
            self.add(x);
        }
    }
}
