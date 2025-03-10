use core::str;
use std::{collections::BTreeSet};

use bit_set::BitSet;
use serde::Serialize;

use crate::{data_reader::DataReader, data_writer::DataWriter, debug_logline, jyutping_splitter::JyutpingSplitter, Dictionary};

#[derive(Debug)]
pub struct CompiledDictionary
{
    character_store : CharacterStore,
    jyutping_store : JyutpingStore,

    entries : Vec<DictionaryEntry>,
    english_data: Vec<u8>,
    english_data_starts: Vec<u32>,
}

pub const FILE_HEADER: &[u8] = b"jyp_dict";
pub const ENGLISH_BLOB_HEADER: &[u8] = b"en_data_";
pub const CURRENT_VERSION: u32 = 2;

impl CompiledDictionary {
    pub fn from_dictionary(dict : Dictionary) -> Self {
        let mut all_characters : BTreeSet<char> = BTreeSet::new();
        let mut all_jyutping_words : BTreeSet<String> = BTreeSet::new();

        for (characters, jyutping) in dict.trad_to_jyutping.inner.iter() {
            for c in characters.chars() {
                all_characters.insert(c);
            }

            // TODO Lowercases?
            for mut word in JyutpingSplitter::new(jyutping) {
                if (word.len() > 0 && word.chars().last().unwrap().is_ascii_digit()) {
                    word = &word[0..word.len() - 1];
                }
                all_jyutping_words.insert(word.to_owned());
            }
        }

        for (characters, _english) in dict.trad_to_def.inner.iter()
        {
            for c in characters.chars()
            {
                all_characters.insert(c);
            }
        }

        let all_characters_list : Vec<char> = all_characters.into_iter().collect();
        let all_jyutping_words_list : Vec<String> = all_jyutping_words.into_iter().collect();

        let character_store = CharacterStore::from_chars(all_characters_list);
        let jyutping_store = JyutpingStore::from_strings(all_jyutping_words_list);

        debug_log!("Individual characters {}, Individual jyutping words {}", character_store.characters.len(), jyutping_store.base_strings.len());
        //println!("{:#?}", character_store.characters);
        //println!("{:#?}", jyutping_store.base_strings);

        let mut entries = Vec::new();

        let mut english_data = Vec::new();
        let mut english_data_starts = Vec::new();

        let mut english_start = 0;

        for (traditional_chars, definitions) in dict.trad_to_def.inner
        {
            let mut cost : u32 = 0;

            let mut char_indexes = Vec::new();

            for character in traditional_chars.chars()
            {
                char_indexes.push(character_store.char_to_index(character).expect(&format!("Could not find match for {}", character)));
                cost += dict.trad_to_frequency.get_or_default(character).cost;
            }

            let mut jyutpings = Vec::new();
            if let Some(jyutping_string) = dict.trad_to_jyutping.inner.get(&traditional_chars)
            {
                for word in JyutpingSplitter::new(jyutping_string)
                {
                    jyutpings.push(jyutping_store.get(word).unwrap());
                }
            }

            let english_start = english_data_starts.len();
            for definition in definitions
            {
                english_data_starts.push(english_data.len() as u32);
                english_data.extend_from_slice(definition.as_bytes());
            }
            let english_end = english_data_starts.len();

            entries.push(DictionaryEntry {
                characters: char_indexes,
                jyutpings: jyutpings,
                english_start: english_start as u32,
                english_end: english_end as u32,
                cost,
            });
        }

        english_data_starts.push(english_data.len() as u32);

        entries.sort_by(|x, y| x.cost.cmp(&y.cost));

        Self {
            character_store,
            jyutping_store,
            entries,
            english_data,
            english_data_starts,
        }
    }
}

pub struct QueryTerms {
    pub jyutping_terms: Vec<JyutpingQueryTerm>,
    pub traditional_terms: Vec<u16>,
}

pub struct JyutpingQueryTerm {
    pub matches: BitSet,
    pub tone: Option<u8>,
    pub match_bit_to_match_cost: Vec<(usize, u32)>,
}

#[derive(Debug)]
pub struct MatchCostInfo
{
    pub match_cost: u32,
    pub static_cost: u32,
}

impl MatchCostInfo {
    pub fn total(&self) -> u32 {
        self.match_cost + self.static_cost
    }
}

impl CompiledDictionary {
    pub fn search(&self, s : &str) -> Vec<(MatchCostInfo, &DictionaryEntry)>
    {
        let mut jyutping_terms = Vec::new();
        for query_term in s.split_ascii_whitespace()
        {
            jyutping_terms.push(self.get_jyutping_query_term(query_term));
        }

        let mut traditional_terms = Vec::new();
        for c in s.chars()
        {
            if let Some(c_id) = self.character_store.char_to_index(c) {
                traditional_terms.push(c_id);
            }
        }

        let query_terms = QueryTerms {
            jyutping_terms,
            traditional_terms,
        };

        let mut matches: Vec<(MatchCostInfo, &DictionaryEntry)> = Vec::new();

        //let max = 16;
        for x in &self.entries
        {
            if let Some(match_cost) = self.matches_query_jyutping(x, &query_terms)
            {
                let cost_info = MatchCostInfo {
                    match_cost,
                    static_cost: x.cost,
                };

                matches.push((cost_info, x));

                //if (matches.len() >= max)
                //{
                //    break;
                //}
            }
            else
            {
                let force_english = false;
                if (s.len() > 2 || force_english)
                {
                    if let Some(match_cost) = self.matches_query_english(x, s)
                    {
                        let cost_info = MatchCostInfo {
                            match_cost,
                            static_cost: x.cost,
                        };

                        matches.push((cost_info, x));
                    }
                }

                if (!query_terms.traditional_terms.is_empty())
                {
                    if (self.matches_query_traditional(x, &query_terms)) {
                        let cost_info = MatchCostInfo {
                            match_cost: 0,
                            static_cost: x.cost,
                        };

                        matches.push((cost_info, x));
                    }
                }
            }
        }

        debug_log!("Internal candidates: {}", matches.len());
        matches.sort_by(|(x, _), (y, _)| x.total().cmp(&y.total()));
        matches.truncate(8);

        matches
    }

    pub fn matches_query_jyutping(&self, entry: &DictionaryEntry, query_terms : &QueryTerms) -> Option<u32>
    {
        if (entry.jyutpings.len() < query_terms.jyutping_terms.len()) {
            return None;
        }

        let mut match_cost = 0;

        let mut entry_jyutping_matches = BitSet::new();

        for jyutping_term in &query_terms.jyutping_terms
        {
            let mut term_match = false;

            for (i, entry_jyutping) in entry.jyutpings.iter().enumerate()
            {
                if (jyutping_term.matches.contains(entry_jyutping.base as usize))
                {
                    let mut term_match_cost = 0;
                    for (match_bit, cost) in &jyutping_term.match_bit_to_match_cost {
                        if (*match_bit == entry_jyutping.base as usize) {
                            term_match_cost = *cost;
                            // Break out of finding term_match_cost.
                            break;
                        }
                    }

                    if let Some(t) = jyutping_term.tone
                    {
                        if t == entry_jyutping.tone
                        {
                            term_match = true;
                        }
                    }
                    else
                    {
                        term_match = true;
                    }

                    if (term_match)
                    {
                        match_cost += term_match_cost;
                        entry_jyutping_matches.insert(i);
                        break;
                    }
                }
            }

            if (!term_match)
            {
                return None;
            }
        }

        //let additional_terms = entry.jyutpings.len() - query_terms.jyutping_matches.len();
        //match_cost += additional_terms as u32 * 10_000;

        for i in 0..entry.jyutpings.len() {
            if (!entry_jyutping_matches.contains(i)) {
                match_cost += ((entry.jyutpings.len() + 1) - i) as u32 * 10_000;
            }
        }

        Some(match_cost)
    }

    pub fn matches_query_english(&self, entry: &DictionaryEntry, s : &str) -> Option<u32>
    {
        // Make sure we prefer jyutping matches
        let mut cost: u32 = 1_000;

        // @Perf do search over enterity instead of individual entries.

        if (entry.english_start == entry.english_end)
        {
            return None;
        }

        unsafe {
            let start = self.english_data_starts[entry.english_start as usize] as usize;
            let end = self.english_data_starts[entry.english_end as usize] as usize;
            let block = &self.english_data[start..end];
            let block_str = unsafe {
                str::from_utf8_unchecked(block)
            };

            'outer: for split in s.split_ascii_whitespace()
            {
                /*
                for (i, def) in entry.english_definitions.iter().enumerate()
                {
                    if let Some(pos) = crate::string_search::string_indexof_linear_ignorecase(split, def) {
                        cost += i as u32 * 1_000;
                        cost += pos as u32 * 100;
                        continue 'outer;
                    }
                }
                */

                // @FIXME entry boundaries etc.

                if let Some(pos) = crate::string_search::string_indexof_linear_ignorecase(split, block_str) {
                    //cost += i as u32 * 1_000;
                    cost += pos as u32 * 100;
                    continue 'outer;
                }

                // No match on this split
                return None;
            }
        }

        Some(cost)
    }

    pub fn matches_query_traditional(&self, entry: &DictionaryEntry, query_terms : &QueryTerms) -> bool
    {
        for c_id in query_terms.traditional_terms.iter()
        {
            if (!entry.characters.contains(c_id))
            {
                return false
            }
        }

        true
    }

    pub fn get_jyutping_query_term(&self, mut s : &str) -> JyutpingQueryTerm
    {
        let mut tone : Option<u8> = None;

        let bs = s.as_bytes();
        if (bs.len() > 0)
        {
            if bs[bs.len() - 1].is_ascii_digit() {
                tone = Some(bs[bs.len() - 1] as u8 - '0' as u8);
                s = unsafe { std::str::from_utf8_unchecked(&bs[0..bs.len()-1])};
            }
        }

        let mut matches = BitSet::new();
        let mut match_bit_to_match_cost = Vec::new();

        for (i, jyutping_string) in self.jyutping_store.base_strings.iter().enumerate()
        {
            if (jyutping_string.eq_ignore_ascii_case(s))
            {
                debug_log!("'{}' matches {}", s, jyutping_string);
                matches.insert(i);
                continue;
            }

            if let Some(_) = crate::string_search::string_indexof_linear_ignorecase(s, &jyutping_string)
            {
                let match_cost = (jyutping_string.len() - s.len()) as u32 * 6_000;
                debug_log!("'{}' matches {} with cost {}", s, jyutping_string, match_cost);
                match_bit_to_match_cost.push((i, match_cost));
                matches.insert(i);
                continue;
            }

            /*
            // Too noisy

            let dist = crate::string_search::prefix_levenshtein_ascii(s, &jyutping_string);
            if (dist < 2) {
                let match_cost = dist as u32 * 10_000;
                println!("'{}' fuzzy matches {} with cost {}", s, jyutping_string, match_cost);
                match_bit_to_match_cost.push((i, match_cost));
                matches.insert(i);
                continue;
            }
            */
        }

        JyutpingQueryTerm {
            matches,
            tone,
            match_bit_to_match_cost,
        }
    }

    pub fn deserialize(reader : &mut DataReader) -> Self {
        let header = reader.read_bytes_len(8);
        debug_log!("Header '{}'", std::str::from_utf8(header).expect("Not utf8"));
        assert!(header == FILE_HEADER);

        let version = reader.read_u32();
        debug_log!("Version {}", version);
        assert_eq!(CURRENT_VERSION, version);

        let mut character_store = CharacterStore::default();
        let character_count = reader.read_u32();
        for _ in 0..character_count {
            character_store.characters.push(reader.read_utf8_char());
        }

        let mut jyutping_store = JyutpingStore::default();
        let jyutping_count = reader.read_u32();
        for _ in 0..jyutping_count {

            // TODO move to offset_string
            let base_string = reader.read_string().to_owned();
            jyutping_store.base_strings.push(base_string);
        }

        let entry_count = reader.read_u32();
        let mut entries = Vec::with_capacity(entry_count as usize);

        for _ in 0..entry_count {
            let mut entry = DictionaryEntry::default();

            let char_count = reader.read_u8();
            for _ in 0..char_count {
                entry.characters.push(reader.read_vbyte() as u16);
            }

            let jyutping_count = reader.read_u8();
            for _ in 0..jyutping_count {
                let base = reader.read_vbyte() as u16;
                let tone = reader.read_u8();
                entry.jyutpings.push(Jyutping { base, tone });
            }

            entry.english_start = reader.read_u32();
            entry.english_end = reader.read_u32();

            entry.cost = reader.read_u32();

            entries.push(entry);
        }

        let blob_header = reader.read_bytes_len(8);
        debug_log!("blob_header '{}'", std::str::from_utf8(blob_header).expect("Not utf8"));
        assert!(blob_header == ENGLISH_BLOB_HEADER);

        let blob_size = reader.read_u32();
        let mut english_blob = reader.read_bytes_len(blob_size as usize);

        let mut starts_count = reader.read_u32() as usize;
        let mut english_data_starts = Vec::with_capacity(starts_count);
        for _ in 0..starts_count
        {
            let start = reader.read_vbyte();
            english_data_starts.push(start as u32);
        }

        Self {
            character_store,
            jyutping_store,
            entries,
            english_data: english_blob.to_owned(),
            english_data_starts,
        }
    }

    pub fn serialize<T : std::io::Write>(&self, writer : &mut DataWriter<T>) -> std::io::Result<()>
    {
        writer.write_bytes(FILE_HEADER)?;
        writer.write_u32(CURRENT_VERSION)?;

        writer.write_u32(self.character_store.characters.len() as u32)?;
        for c in &self.character_store.characters
        {
            writer.write_utf8(*c)?;
        }

        writer.write_u32(self.jyutping_store.base_strings.len() as u32)?;
        for j in &self.jyutping_store.base_strings
        {
            writer.write_string(j)?;
        }

        writer.write_u32(self.entries.len() as u32)?;
        for e in &self.entries
        {
            assert!(e.characters.len() < 256);
            writer.write_u8(e.characters.len() as u8)?;
            for c in &e.characters
            {
                writer.write_vbyte(*c as u64)?;
            }

            assert!(e.jyutpings.len() < 256);
            writer.write_u8(e.jyutpings.len() as u8)?;
            for j in &e.jyutpings
            {
                writer.write_vbyte(j.base as u64)?;
                writer.write_u8(j.tone)?;
            }

            writer.write_u32(e.english_start as u32)?;
            writer.write_u32(e.english_end as u32)?;

            writer.write_u32(e.cost)?;
        }

        writer.write_bytes(ENGLISH_BLOB_HEADER)?;

        writer.write_bytes_and_length(&self.english_data)?;

        writer.write_u32(self.english_data_starts.len() as u32)?;
        for start in &self.english_data_starts
        {
            writer.write_vbyte(*start as u64)?;
        }

        // End padding
        writer.write_u64(0);

        Ok(())
    }
}

#[derive(Default, Debug)]
struct CharacterStore
{
    characters : Vec<char>,
    //to_jyutping : Vec<u16>,
}

impl CharacterStore
{
    pub fn from_chars(mut characters : Vec<char>) -> Self
    {
        characters.sort();

        Self {
            characters,
        }
    }
    pub fn char_to_index(&self, c : char) -> Option<u16>
    {
        self.characters.binary_search(&c).map(|x| x as u16).ok()
    }
}

#[derive(Default, Debug)]
struct JyutpingStore
{
    base_strings : Vec<String>,
    //to_character : Vec<u16>,
}


impl JyutpingStore {
    pub fn from_strings(mut jyutpings : Vec<String>) -> Self
    {
        jyutpings.sort();

        Self {
            base_strings: jyutpings,
        }
    }

    pub fn get(&self, word_with_tone : &str) -> Option<Jyutping>
    {
        let bs = word_with_tone.as_bytes();
        assert!(bs.len() > 0);

        let last = bs[bs.len() - 1];
        assert!(last.is_ascii_digit());

        let tone = last - ('0' as u8);
        let without_tone = unsafe { std::str::from_utf8_unchecked(&bs[0..bs.len()-1]) };

        self.get_with_tone(without_tone, tone)
    }

    pub fn get_with_tone(&self, word : &str, tone : u8) -> Option<Jyutping>
    {
        self.base_strings.binary_search_by(|x| (&x[..]).cmp(word)).map(|x| Jyutping
            {
                base: x as u16,
                tone,
            }).ok()
    }

    //pub fn matches(&self, jyutping: Jyutping, base : &str, tone : Option<u8>) -> bool {
    //    if let Some(t) = tone {
    //        if (jyutping.tone != t) {
    //            return false;
    //        }
    //    }

    //    let base_str = &self.base_strings[jyutping.base as usize];
    //    base_str.contains(base)
    //}
}

#[derive(Debug, Clone, Copy)]
pub struct Jyutping
{
    // TODO merge to single u16
    pub base : u16,
    pub tone : u8,
}


#[derive(Debug, Clone, Default)]
pub struct DictionaryEntry
{
    pub characters : Vec<u16>,
    // TODO struct of array members here
    pub jyutpings : Vec<Jyutping>,
    pub english_start : u32,
    pub english_end : u32,
    pub cost : u32,
}

pub struct Result
{
    entry_index : usize,
    base_cost : f32,
    match_cost : f32,
}


#[derive(Debug, Serialize)]
pub struct DisplayDictionaryEntry
{
    pub characters : String,
    pub jyutping : String,
    pub english_definitions : Vec<String>,
    pub cost : u32,
}

impl DisplayDictionaryEntry
{
    pub fn from_entry(entry : &DictionaryEntry, dict : &CompiledDictionary) -> Self {
        let mut characters = String::new();
        for c in &entry.characters
        {
            characters.push(dict.character_store.characters[*c as usize]);
        }

        let mut jyutping = String::new();
        for j in &entry.jyutpings
        {
            if (jyutping.len() > 0)
            {
                jyutping.push(' ');
            }

            jyutping.push_str(&dict.jyutping_store.base_strings[j.base as usize]);
            jyutping.push((j.tone + '0' as u8) as char);
        }

        let mut english_definitions = Vec::with_capacity(entry.english_end as usize - entry.english_start as usize);
        for i in entry.english_start..entry.english_end
        {
            let start = dict.english_data_starts[i as usize] as usize;
            let end = dict.english_data_starts[i as usize + 1] as usize;
            let blob = &dict.english_data[start..end];
            let def = unsafe { std::str::from_utf8_unchecked(blob) }.to_owned();
            english_definitions.push(def);
        }

        Self {
            characters,
            jyutping,
            english_definitions,
            cost : entry.cost,
        }
    }
}