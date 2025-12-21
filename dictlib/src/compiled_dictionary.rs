use core::str;
use std::{collections::BTreeSet, io::BufWriter};
use std::io::Write;

use bit_set::BitSet;
use serde::Serialize;

use crate::EntrySource;
use crate::{data_reader::DataReader, data_writer::DataWriter, debug_logline, jyutping_splitter::JyutpingSplitter, Dictionary};

#[derive(Debug)]
pub struct CompiledDictionary
{
    character_store : CharacterStore,
    jyutping_store : JyutpingStore,

    entries : Vec<CompiledDictionaryEntry>,
    english_data: Vec<u8>,
    english_data_starts: Vec<u32>,
}

pub const FILE_HEADER: &[u8] = b"jyp_dict";
pub const ENGLISH_BLOB_HEADER: &[u8] = b"en_data_";
pub const CURRENT_VERSION: u32 = 8;

impl CompiledDictionary {
    pub fn from_dictionary(mut dict : Dictionary) -> Self {
        let mut all_characters : BTreeSet<char> = BTreeSet::new();
        let mut all_jyutping_words : BTreeSet<String> = BTreeSet::new();

        for entry in dict.entries.iter() {
            for c in entry.traditional.chars() {
                all_characters.insert(c);
            }

            // TODO Lowercases?
            for mut word in JyutpingSplitter::new(&entry.jyutping) {
                if (word.len() > 0 && word.chars().last().unwrap().is_ascii_digit()) {
                    word = &word[0..word.len() - 1];
                }

                all_jyutping_words.insert(word.to_owned());
            }

            for c in entry.traditional.chars()
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

        dict.entries.sort_by(|x, y| x.cost.cmp(&y.cost));

        let mut entries = Vec::new();

        let mut english_data = Vec::new();
        let mut english_data_starts = Vec::new();

        for entry in &dict.entries
        {
            let mut flags: u8 = 0;
            match entry.source {
                EntrySource::CEDict => {
                    flags |= FLAG_SOURCE_CEDICT;
                }
                EntrySource::CCanto => {
                    flags |= FLAG_SOURCE_CCCANTO;
                }
            }

            let mut char_indexes = Vec::new();
            for character in entry.traditional.chars()
            {
                char_indexes.push(character_store.char_to_index(character).expect(&format!("Could not find match for {}", character)));
                //cost += dict.trad_to_frequency.get_or_default(character).cost;
            }

            let mut mapped_jyutping = Vec::new();
            for word in JyutpingSplitter::new(&entry.jyutping)
            {
                mapped_jyutping.push(jyutping_store.get(word).unwrap());
            }

            let english_start = english_data_starts.len();
            for definition in &entry.english_sets.inner
            {
                english_data_starts.push(english_data.len() as u32);
                english_data.extend_from_slice(definition.as_bytes());
            }
            let english_end = english_data_starts.len();

            entries.push(CompiledDictionaryEntry {
                characters: char_indexes,
                jyutping: mapped_jyutping,
                english_start: english_start as u32,
                english_end: english_end as u32,
                cost: entry.cost,
                flags,
            });
        }

        english_data_starts.push(english_data.len() as u32);

        Self {
            character_store,
            jyutping_store,
            entries,
            english_data,
            english_data_starts,
        }
    }

    pub fn get_diplay_entry(&self, i: usize) -> DisplayDictionaryEntry {
        DisplayDictionaryEntry::from_entry(&self.entries[i], self)
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

#[derive(Debug, Serialize)]
pub struct MatchCostInfo
{
    pub term_match_cost: u32,
    pub unmatched_position_cost: u32,
    pub inversion_cost: u32,
    pub static_cost: u32,
}

impl MatchCostInfo {
    pub fn total(&self) -> u32 {
        self.term_match_cost + self.unmatched_position_cost + self.inversion_cost + self.static_cost
    }
}

#[derive(Debug, Serialize)]
pub enum MatchType {
    Jyutping,
    Traditional,
    English,
}

#[derive(Debug, Serialize)]
pub struct Match
{
    pub cost_info : MatchCostInfo,
    pub match_type: MatchType,
    pub entry_id: usize,
}

impl CompiledDictionary {
    pub fn search(&self, s : &str) -> Vec<Match>
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

        let mut matches: Vec<Match> = Vec::new();

        //let max = 16;
        for (i, x) in self.entries.iter().enumerate()
        {
            if let Some(mut cost_info) = self.matches_jyutping_term(x, &query_terms)
            {
                cost_info.static_cost = x.cost;

                matches.push(Match {
                    cost_info,
                    match_type: MatchType::Jyutping,
                    entry_id: i,
                });

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
                    if let Some(mut match_cost) = self.matches_query_english(x, s)
                    {
                        for c in s.chars() {
                            if (!c.is_ascii()) {
                                // Non-ascii match, probably a chinese character
                                // match within an english description
                                match_cost += 8_000;
                            }
                        }

                        let cost_info = MatchCostInfo {
                            term_match_cost: match_cost,
                            unmatched_position_cost: 0,
                            inversion_cost: 0,
                            static_cost: x.cost,
                        };

                        matches.push(Match {
                            cost_info,
                            match_type: MatchType::English,
                            entry_id: i,
                        });
                    }
                }

                if (!query_terms.traditional_terms.is_empty())
                {
                    if (self.matches_query_traditional(x, &query_terms)) {
                        let cost_info = MatchCostInfo {
                            term_match_cost: 0,
                            unmatched_position_cost: 0,
                            inversion_cost: 0,
                            static_cost: x.cost,
                        };

                        matches.push(Match {
                            cost_info,
                            match_type: MatchType::Traditional,
                            entry_id: i,
                        });
                    }
                }
            }
        }

        debug_log!("Internal candidates: {}", matches.len());
        matches.sort_by(|(x), (y)| x.cost_info.total().cmp(&y.cost_info.total()));
        matches.truncate(8);

        matches
    }

    pub fn matches_jyutping_term(&self, entry: &CompiledDictionaryEntry, query_terms : &QueryTerms) -> Option<MatchCostInfo> {
        if (entry.jyutping.len() < query_terms.jyutping_terms.len()) {
            return None;
        }

        let mut total_term_match_cost = 0;

        let mut entry_jyutping_matches = BitSet::new();
        let mut matched_positions: Vec<usize> = Vec::new();

        for jyutping_term in &query_terms.jyutping_terms
        {
            let mut term_match = false;

            for (i, entry_jyutping) in entry.jyutping.iter().enumerate()
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
                        total_term_match_cost += term_match_cost;
                        entry_jyutping_matches.insert(i);
                        matched_positions.push(i);
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

        let mut inversion_cost = 0u32;
        for i in 0..matched_positions.len() {
            for j in (i + 1)..matched_positions.len() {
                if matched_positions[i] > matched_positions[j] {
                    const OUT_OF_ORDER_INVERSION_PENALTY: u32 = 5000;
                    inversion_cost += OUT_OF_ORDER_INVERSION_PENALTY;
                }
            }
        }

        let mut unmatched_position_cost = 0u32;
        for i in 0..entry.jyutping.len() {
            if (!entry_jyutping_matches.contains(i)) {
                unmatched_position_cost += ((entry.jyutping.len() + 1) - i) as u32 * 10_000;
            }
        }

        Some(MatchCostInfo {
            term_match_cost: total_term_match_cost,
            unmatched_position_cost,
            inversion_cost,
            static_cost: 0,
        })
    }

    pub fn matches_query_english(&self, entry: &CompiledDictionaryEntry, s : &str) -> Option<u32>
    {
        // Make sure we prefer jyutping matches
        let mut cost: u32 = 1_000;

        // @Perf do search over enterity instead of individual entries.

        if (entry.english_start == entry.english_end)
        {
            return None;
        }

        let start = self.english_data_starts[entry.english_start as usize] as usize;
        let end = self.english_data_starts[entry.english_end as usize] as usize;
        let block = &self.english_data[start..end];

        for split in s.split_ascii_whitespace()
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

            if let Some(pos) = crate::string_search::string_indexof_linear_ignorecase(split, block) {
                //cost += i as u32 * 1_000;
                cost += pos as u32 * 100;

                if (pos == 0)  {
                    continue;
                }

                let start_c = block[pos-1];
                if (start_c.is_ascii_whitespace() || start_c == b'-')
                {
                    continue;
                }

                // Match in the middle of a word
                cost += 5_000;
                continue;
            }

            // No match on this split
            return None;
        }

        Some(cost)
    }

    pub fn matches_query_traditional(&self, entry: &CompiledDictionaryEntry, query_terms : &QueryTerms) -> bool
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

            if let Some(_) = crate::string_search::string_indexof_linear_ignorecase(s, jyutping_string.as_bytes())
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

        let mut english_start = 0;
        let mut prev_cost = 0;
        for entry_id in 0..entry_count {
            let mut entry = CompiledDictionaryEntry::default();
            entry.flags = reader.read_u8();

            let char_count = reader.read_u8();
            for _ in 0..char_count {
                entry.characters.push(reader.read_u16() as u16);
            }

            let jyutping_count = reader.read_u8();
            entry.jyutping.reserve(jyutping_count as usize);
            for _ in 0..jyutping_count {
                entry.jyutping.push(Jyutping::unpack(reader.read_u16()));
            }

            entry.english_start = english_start;
            entry.english_end = english_start + reader.read_u8() as u32;
            english_start = entry.english_end;

            let cost_delta = reader.read_vbyte() as u32;
            entry.cost = prev_cost + cost_delta;
            prev_cost = entry.cost;

            entries.push(entry);
        }

        let blob_header = reader.read_bytes_len(8);
        debug_log!("blob_header '{}'", std::str::from_utf8(blob_header).expect("Not utf8"));
        assert!(blob_header == ENGLISH_BLOB_HEADER);

        let blob_size = reader.read_u32();
        let english_blob = reader.read_bytes_len(blob_size as usize);

        let starts_count = reader.read_u32() as usize;
        let mut english_data_starts = Vec::with_capacity(starts_count);

        let mut prev_start = 0;
        for _ in 0..starts_count
        {
            let delta = reader.read_vbyte();
            let start = prev_start + delta;
            prev_start = start;
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

    pub fn dump_entries(&self, path: &str) {
        let file = std::fs::File::create(path).unwrap();
        let mut writer = BufWriter::new(file);
        for entry in &self.entries {
            let e = DisplayDictionaryEntry::from_entry(entry, self);
            write!(writer, "{:#?}\n", e).unwrap();
        }
    }

    pub fn serialize<T : std::io::Write>(&self, writer : &mut DataWriter<T>) -> std::io::Result<()>
    {
        println!("Writing Header");
        writer.write_bytes(FILE_HEADER)?;
        println!("Writing Version = {}", CURRENT_VERSION);
        writer.write_u32(CURRENT_VERSION)?;

        {
            let start = writer.write_len;
            let characters_len = self.character_store.characters.len() as u32;
            writer.write_u32(characters_len)?;
            println!("Writing Characters, length = {}", characters_len);
            for c in &self.character_store.characters
            {
                writer.write_utf8(*c)?;
            }

            let bytes = writer.write_len - start;
            println!("Characters bytes = {}", bytes);
        }

        {
            let start = writer.write_len;

            let juytping_strings_len = self.jyutping_store.base_strings.len() as u32;
            writer.write_u32(juytping_strings_len)?;
            println!("Writing Jyutping, length = {}", juytping_strings_len);
            for j in &self.jyutping_store.base_strings
            {
                writer.write_string(j)?;
            }

            let bytes = writer.write_len - start;
            println!("Jyutping bytes = {}", bytes);
        }

        {
            let start = writer.write_len;

            println!("Writing entries, {} entries", self.entries.len());
            writer.write_u32(self.entries.len() as u32)?;
            let mut prev_english_start = 0;
            let mut prev_cost = 0;
            for (i, e) in self.entries.iter().enumerate()
            {
                writer.write_u8(e.flags)?;

                assert!(e.characters.len() < 128);
                writer.write_u8(e.characters.len() as u8)?;
                for c in &e.characters
                {
                    writer.write_u16(*c)?;
                }

                assert!(e.jyutping.len() < 256);
                writer.write_u8(e.jyutping.len() as u8)?;
                for j in &e.jyutping {
                    writer.write_u16(j.pack())?;
                }

                assert!(prev_english_start <= e.english_start);
                prev_english_start = e.english_start;
                writer.write_u8((e.english_end - e.english_start) as u8)?;

                let cost_delta = e.cost - prev_cost;
                writer.write_vbyte(cost_delta as u64)?;
                prev_cost = e.cost;
            }

            let bytes = writer.write_len - start;
            println!("Entries bytes = {}", bytes);
        }

        writer.write_bytes(ENGLISH_BLOB_HEADER)?;

        {
            let start = writer.write_len;

            println!("Writing english data, length = {}", self.english_data.len());
            writer.write_bytes_and_length(&self.english_data)?;

            let bytes = writer.write_len - start;
            println!("English data bytes = {}", bytes);
        }

        {
            let start = writer.write_len;

            println!("Writing english data starts, length = {}", self.english_data_starts.len());
            let mut prev_start = 0;
            writer.write_u32(self.english_data_starts.len() as u32)?;
            for start in &self.english_data_starts
            {
                let delta = *start - prev_start;
                prev_start = *start;
                writer.write_vbyte(delta as u64)?;
            }

            let bytes = writer.write_len - start;
            println!("English starts bytes = {}", bytes);
        }

        // End padding
        writer.write_u64(0)?;

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

impl Jyutping {
    pub fn pack(self) -> u16 {
        const K : u16 = 1 << 13;
        assert!(self.base < K);

        assert!(self.tone <= 6);
        let packed_base_with_tone = self.base | ((self.tone as u16) << 13);
        packed_base_with_tone
    }

    pub fn unpack(packed: u16) -> Self {
        let base = packed & 0x0FFF;
        let tone = (packed & 0xE000) >> 13;
        if (tone > 6) {
            panic!("Bad tone {} - base {}, packed {:#01x}", tone, base, packed);
        }
        assert!(tone <= 6);
        let base = base as u16;
        let tone = tone as u8;

        Self {
            base,
            tone,
        }
    }
}


#[derive(Debug, Clone, Default)]
pub struct CompiledDictionaryEntry
{
    pub characters : Vec<u16>,
    // TODO struct of array members here
    pub jyutping : Vec<Jyutping>,
    pub english_start : u32,
    pub english_end : u32,
    pub cost : u32,
    pub flags: u8,
}

const FLAG_SOURCE_CEDICT: u8 = 0x1;
const FLAG_SOURCE_CCCANTO: u8 = 0x2;

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
    pub entry_source: EntrySource,
}

impl DisplayDictionaryEntry
{
    pub fn from_entry(entry : &CompiledDictionaryEntry, dict : &CompiledDictionary) -> Self {
        let mut characters = String::new();
        for c in &entry.characters
        {
            characters.push(dict.character_store.characters[*c as usize]);
        }

        let mut jyutping = String::new();
        for j in &entry.jyutping {
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

        let entry_source = if entry.flags & FLAG_SOURCE_CEDICT != 0 {
            EntrySource::CEDict
        }
        else if entry.flags & FLAG_SOURCE_CCCANTO != 0 {
            EntrySource::CCanto
        }
        else {
            panic!("Unknown data source");
        };

        Self {
            characters,
            jyutping,
            english_definitions,
            cost : entry.cost,
            entry_source,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Jyutping;

    #[test]
    pub fn pack_unpack() {

        for j in [Jyutping{
            tone: 2,
            base: 1024,
        }, Jyutping {
            tone: 1,
            base: 10,
        }, Jyutping {
            tone: 6,
            base: 0,
        }, Jyutping {
            tone: 5,
            base: 251,
        }] {
            let packed = j.pack();
            let unpacked = Jyutping::unpack(packed);

            assert_eq!(unpacked.tone, j.tone);
            assert_eq!(unpacked.base, j.base);
        }
    }
}
