use core::str;
use std::{collections::BTreeSet, io::BufWriter};
use std::io::Write;

use serde::Serialize;

use crate::EntrySource;
use crate::{data_reader::DataReader, data_writer::DataWriter, jyutping_splitter::JyutpingSplitter, builder::Builder};

#[derive(Debug)]
pub struct CompiledDictionary
{
    pub character_store : CharacterStore,
    pub jyutping_store : JyutpingStore,

    pub entries : Vec<CompiledDictionaryEntry>,
    pub english_data: Vec<u8>,
    pub english_data_starts: Vec<u32>,
}

pub const FILE_HEADER: &[u8] = b"jyp_dict";
pub const ENGLISH_BLOB_HEADER: &[u8] = b"en_data_";
pub const CURRENT_VERSION: u32 = 8;

impl CompiledDictionary {
    pub fn from_builder(mut dict : Builder) -> Self {
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
                char_indexes.push(character_store.char_to_index(character).unwrap_or_else(|| panic!("Could not find match for {}", character)));
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

    pub fn deserialize(reader : &mut DataReader) -> Self {
        let header = reader.read_bytes_len(8);
        debug_log!("Header '{}'", std::str::from_utf8(header).expect("Header not utf8"));
        assert_eq!(header, FILE_HEADER);

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
        for _ in 0..entry_count {
            let mut entry = CompiledDictionaryEntry {
                flags: reader.read_u8(),
                ..Default::default()
            };

            let char_count = reader.read_u8();
            for _ in 0..char_count {
                entry.characters.push(reader.read_u16());
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
            writeln!(writer, "{:#?}", e).unwrap();
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
            for e in &self.entries
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

    pub fn get_display_entry(&self, i: usize) -> DisplayDictionaryEntry {
        DisplayDictionaryEntry::from_entry(&self.entries[i], self)
    }

    // Typo backward compatibility - kept for console/tests
    pub fn get_diplay_entry(&self, i: usize) -> DisplayDictionaryEntry {
        self.get_display_entry(i)
    }
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
pub struct CharacterStore
{
    pub characters : Vec<char>,
}

#[derive(Default, Debug)]
pub struct JyutpingStore
{
    pub base_strings : Vec<String>,
}

impl JyutpingStore {
    pub fn from_strings(mut jyutpings : Vec<String>) -> Self
    {
        jyutpings.sort();

        Self {
            base_strings: jyutpings,
        }
    }

    pub fn get_string(&self, j: Jyutping) -> String {
        let str = &self.base_strings[j.base as usize];
        let mut string = String::with_capacity(str.len() + 1);
        string.push_str(str);
        string.push((j.tone + b'0') as char);
        string
    }

    pub fn get(&self, word_with_tone : &str) -> Option<Jyutping>
    {
        let bs = word_with_tone.as_bytes();
        assert!(bs.len() > 0);

        let last = bs[bs.len() - 1];
        assert!(last.is_ascii_digit());

        let tone = last - b'0';
        let without_tone = unsafe { std::str::from_utf8_unchecked(&bs[0..bs.len()-1]) };

        self.get_with_tone(without_tone, tone)
    }

    pub fn get_with_tone(&self, word : &str, tone : u8) -> Option<Jyutping>
    {
        self.base_strings.binary_search_by(|x| x[..].cmp(word)).map(|x| Jyutping
            {
                base: x as u16,
                tone,
            }).ok()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Jyutping
{
    // @Perf merge to single u16, don't need all the bits for base
    pub base : u16,
    pub tone : u8,
}

impl Jyutping {
    pub fn pack(self) -> u16 {
        const K : u16 = 1 << 13;
        assert!(self.base < K);

        assert!(self.tone <= 6);

        self.base | ((self.tone as u16) << 13)
    }

    pub fn unpack(packed: u16) -> Self {
        let base = packed & 0x0FFF;
        let tone = (packed & 0xE000) >> 13;
        if (tone > 6) {
            panic!("Bad tone {} - base {}, packed {:#01x}", tone, base, packed);
        }
        assert!(tone <= 6);
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

impl CompiledDictionaryEntry
{
    pub fn get_source(&self) -> EntrySource {
        if self.flags & FLAG_SOURCE_CEDICT != 0 {
            EntrySource::CEDict
        } else if self.flags & FLAG_SOURCE_CCCANTO != 0 {
            EntrySource::CCanto
        } else {
            panic!("Unknown data source");
        }
    }
}

pub const FLAG_SOURCE_CEDICT: u8 = 0x1;
pub const FLAG_SOURCE_CCCANTO: u8 = 0x2;

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
            jyutping.push((j.tone + b'0') as char);
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

        let entry_source = entry.get_source();

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
pub mod tests {
    use crate::Stopwatch;
    use crate::search::{JYUTPING_COMPLETION_PENALTY_K, JYUTPING_PARTIAL_MATCH_PENALTY_K, JyutpingQueryTerm, MatchType, QueryTerms};

    use super::*;

    struct TestStopwatch;

    impl Stopwatch for TestStopwatch {
        fn elapsed_ms(&self) -> i32 {
            0
        }
    }

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

    pub fn create_test_dict() -> CompiledDictionary {
        // Create a minimal test dictionary
        // Characters must be in sorted order!
        let character_store = CharacterStore {
            characters: vec!['學', '師', '生', '老'], // sorted order
        };

        let jyutping_store = JyutpingStore {
            base_strings: vec![
                "hok".to_string(),
                "lou".to_string(),
                "saang".to_string(),
                "si".to_string(),
            ],
        };

        let entries = vec![
            CompiledDictionaryEntry {
                characters: vec![3, 1], // 老師 (老=index 3, 師=index 1)
                jyutping: vec![
                    Jyutping { base: 1, tone: 5 }, // lou5 (lou=index 1)
                    Jyutping { base: 3, tone: 1 }, // si1 (si=index 3)
                ],
                english_start: 0,
                english_end: 1,
                cost: 100,
                flags: FLAG_SOURCE_CEDICT,
            },
            CompiledDictionaryEntry {
                characters: vec![0, 2], // 學生 (學=index 0, 生=index 2)
                jyutping: vec![
                    Jyutping { base: 0, tone: 6 }, // hok6 (hok=index 0)
                    Jyutping { base: 2, tone: 1 }, // saang1 (saang=index 2)
                ],
                english_start: 1,
                english_end: 2,
                cost: 100,
                flags: FLAG_SOURCE_CEDICT,
            },
        ];

        let english_data = b"teacherstudent".to_vec();
        let english_data_starts = vec![0, 7, 14];

        CompiledDictionary {
            character_store,
            jyutping_store,
            entries,
            english_data,
            english_data_starts,
        }
    }

    #[test]
    fn test_display_entry_format() {
        let dict = create_test_dict();
        let display = dict.get_diplay_entry(0);

        // Verify the jyutping display format includes tone numbers
        assert_eq!(display.jyutping, "lou5 si1");
        assert_eq!(display.characters, "老師");
    }

    #[test]
    fn test_jyutping_query_term_exact_match() {
        let dict = create_test_dict();

        // Test exact match (case insensitive)
        let query_term = JyutpingQueryTerm::create("lou", &dict.jyutping_store);
        assert!(query_term.matches.contains(1)); // "lou" is at index 1
        assert_eq!(query_term.tone, None);

        // Should have no match cost for exact match
        assert_eq!(query_term.match_bit_to_match_cost.len(), 0);
    }

    #[test]
    fn test_jyutping_query_term_with_tone() {
        let dict = create_test_dict();

        // Test query with tone digit
        let query_term = JyutpingQueryTerm::create("lou5", &dict.jyutping_store);
        assert!(query_term.matches.contains(1)); // "lou" is at index 1
        assert_eq!(query_term.tone, Some(5));
    }

    #[test]
    fn test_jyutping_query_term_substring_match() {
        let dict = create_test_dict();

        // Test substring match: "saa" should match "saang"
        let query_term = JyutpingQueryTerm::create("saa", &dict.jyutping_store);
        assert!(query_term.matches.contains(2)); // "saang" is at index 2

        // Should have a match cost penalty
        let cost_entry = query_term.match_bit_to_match_cost.iter()
            .find(|(idx, _)| *idx == 2);
        assert!(cost_entry.is_some(), "Should have cost entry for substring match");

        let (_, cost) = cost_entry.unwrap();
        assert_eq!(*cost, 2 * JYUTPING_COMPLETION_PENALTY_K);
    }

    #[test]
    fn test_jyutping_query_term_prefix_match() {
        let dict = create_test_dict();

        // Test prefix match: "ho" should match "hok"
        let query_term = JyutpingQueryTerm::create("ho", &dict.jyutping_store);
        assert!(query_term.matches.contains(0)); // "hok" is at index 0

        // Should have a match cost penalty for partial match
        let cost_entry = query_term.match_bit_to_match_cost.iter()
            .find(|(idx, _)| *idx == 0);
        assert!(cost_entry.is_some());

        let (_, cost) = cost_entry.unwrap();
        assert_eq!(*cost, JYUTPING_COMPLETION_PENALTY_K);
    }

    #[test]
    fn test_jyutping_query_term_case_insensitive() {
        let dict = create_test_dict();

        // Test case insensitive matching
        let query_term = JyutpingQueryTerm::create("LOU", &dict.jyutping_store);
        assert!(query_term.matches.contains(1));

        let query_term2 = JyutpingQueryTerm::create("LoU", &dict.jyutping_store);
        assert!(query_term2.matches.contains(1));
    }

    #[test]
    fn test_jyutping_substring_match_integration() {
        let dict = create_test_dict();

        // Search with substring should find entries
        let results = dict.search("saa", 8, Box::new(TestStopwatch)).matches;

        assert!(results.len() > 0, "Should find results for substring 'saa' matching 'saang'");
        assert_eq!(results[0].match_obj.entry_id, 1); // Should match the 學生 entry with saang1
        assert!(matches!(results[0].match_obj.match_type, MatchType::Jyutping));

        // Verify the cost is higher than exact match (due to substring penalty)
        assert!(results[0].match_obj.cost_info.term_match_cost > 0);
    }

    #[test]
    fn test_jyutping_prefix_match_integration() {
        let dict = create_test_dict();

        // Search with prefix
        let results = dict.search("ho", 8, Box::new(TestStopwatch)).matches;

        assert!(results.len() > 0, "Should find results for prefix 'ho' matching 'hok'");
        assert_eq!(results[0].match_obj.entry_id, 1); // Should match the 學生 entry with hok6
        assert!(matches!(results[0].match_obj.match_type, MatchType::Jyutping));
    }

    #[test]
    fn test_jyutping_no_false_matches() {
        let dict = create_test_dict();

        // Query that doesn't match anything
        let query_term = JyutpingQueryTerm::create("xyz", &dict.jyutping_store);
        assert_eq!(query_term.matches.len(), 0, "Should not match non-existent jyutping");

        // Search should return no jyutping matches
        let results = dict.search("xyz", 8, Box::new(TestStopwatch)).matches;

        // May have English matches, but no jyutping matches
        for result in results {
            assert!(!matches!(result.match_obj.match_type, MatchType::Jyutping),
                "Should not have jyutping match for non-existent syllable");
        }
    }

    #[test]
    fn test_jyutping_matched_spans_single_syllable() {
        let dict = create_test_dict();
        let entry = &dict.entries[0];

        // Query for "lou" (without tone) should match first syllable
        let query_terms = QueryTerms {
            jyutping_terms: vec![JyutpingQueryTerm::create("lou", &dict.jyutping_store)],
            traditional_terms: vec![],
        };

        let spans = dict.get_jyutping_matched_spans(entry, &query_terms);

        // Should have one span
        assert_eq!(spans.len(), 1);
        let (start, end) = spans[0];


        // Extract the matched substring from display string
        let display = dict.get_diplay_entry(0);
        let matched_text = &display.jyutping[start..end];

        // The matched span should highlight only the typed portion "lou" (no tone)
        assert_eq!(matched_text, "lou",
            "Expected span to cover only typed 'lou' in '{}', got '{}' (span: {}..{})",
            display.jyutping, matched_text, start, end);
    }

    #[test]
    fn test_jyutping_matched_spans_single_syllable_with_tone() {
        let dict = create_test_dict();
        let entry = &dict.entries[0];

        // Query for "lou5" (with tone) should match first syllable
        let query_terms = QueryTerms {
            jyutping_terms: vec![JyutpingQueryTerm::create("lou5", &dict.jyutping_store)],
            traditional_terms: vec![],
        };

        let spans = dict.get_jyutping_matched_spans(entry, &query_terms);

        // Should have two spans: one for base, one for tone
        assert_eq!(spans.len(), 1);

        let display = dict.get_diplay_entry(0);

        // First span should cover the base "lou"
        let (start1, end1) = spans[0];
        assert_eq!(&display.jyutping[start1..end1], "lou5");
    }

    #[test]
    fn test_jyutping_matched_spans_second_syllable() {
        let dict = create_test_dict();
        let entry = &dict.entries[0];

        // Query for "si" should match second syllable
        let query_terms = QueryTerms {
            jyutping_terms: vec![JyutpingQueryTerm::create("si", &dict.jyutping_store)],
            traditional_terms: vec![],
        };

        let spans = dict.get_jyutping_matched_spans(entry, &query_terms);

        assert_eq!(spans.len(), 1);
        let (start, end) = spans[0];


        let display = dict.get_diplay_entry(0);
        let matched_text = &display.jyutping[start..end];

        // Should match only the typed portion "si" in "lou5 si1"
        assert_eq!(matched_text, "si",
            "Expected span to cover only typed 'si' in '{}', got '{}' (span: {}..{})",
            display.jyutping, matched_text, start, end);
    }

    #[test]
    fn test_jyutping_matched_spans_multiple_syllables() {
        let dict = create_test_dict();
        let entry = &dict.entries[0];

        // Query for both syllables
        let query_terms = QueryTerms {
            jyutping_terms: vec![
                JyutpingQueryTerm::create("lou", &dict.jyutping_store),
                JyutpingQueryTerm::create("si", &dict.jyutping_store),
            ],
            traditional_terms: vec![],
        };

        let spans = dict.get_jyutping_matched_spans(entry, &query_terms);

        assert_eq!(spans.len(), 2);

        let display = dict.get_diplay_entry(0);

        // First span should cover only typed "lou"
        let (start1, end1) = spans[0];
        assert_eq!(&display.jyutping[start1..end1], "lou");

        // Second span should cover only typed "si"
        let (start2, end2) = spans[1];
        assert_eq!(&display.jyutping[start2..end2], "si");
    }

    #[test]
    fn test_jyutping_matched_spans_substring_match() {
        let dict = create_test_dict();
        let entry = &dict.entries[1]; // 學生 with "hok6 saang1"

        // Query with substring "saa" should match "saang"
        let query_terms = QueryTerms {
            jyutping_terms: vec![JyutpingQueryTerm::create("saa", &dict.jyutping_store)],
            traditional_terms: vec![],
        };

        let spans = dict.get_jyutping_matched_spans(entry, &query_terms);

        assert_eq!(spans.len(), 1, "Should have one matched span for substring match");
        let (start, end) = spans[0];


        let display = dict.get_diplay_entry(1);
        let matched_text = &display.jyutping[start..end];

        // We searched for "saa", so only "saa" should be highlighted, not the full syllable
        assert_eq!(matched_text, "saa",
            "Substring match 'saa' should highlight only typed 'saa' in '{}', got '{}' (span: {}..{})",
            display.jyutping, matched_text, start, end);
    }

    #[test]
    fn test_jyutping_matched_spans_prefix_match() {
        let dict = create_test_dict();
        let entry = &dict.entries[1]; // 學生 with "hok6 saang1"

        // Query with prefix "ho" should match "hok"
        let query_terms = QueryTerms {
            jyutping_terms: vec![JyutpingQueryTerm::create("ho", &dict.jyutping_store)],
            traditional_terms: vec![],
        };

        let spans = dict.get_jyutping_matched_spans(entry, &query_terms);

        assert_eq!(spans.len(), 1);
        let (start, end) = spans[0];


        let display = dict.get_diplay_entry(1);
        let matched_text = &display.jyutping[start..end];

        // Prefix match "ho" should highlight only the typed portion "ho"
        assert_eq!(matched_text, "ho",
            "Prefix match 'ho' should highlight only typed 'ho' in '{}', got '{}' (span: {}..{})",
            display.jyutping, matched_text, start, end);
    }

    #[test]
    fn test_jyutping_matched_spans_prefix_with_tone() {
        let dict = create_test_dict();
        let entry = &dict.entries[1];

        // Query with exact match and tone "hok6"
        let query_terms = QueryTerms {
            jyutping_terms: vec![JyutpingQueryTerm::create("hok6", &dict.jyutping_store)],
            traditional_terms: vec![],
        };

        let spans = dict.get_jyutping_matched_spans(entry, &query_terms);

        // Should have two spans: one for base, one for tone
        assert_eq!(spans.len(), 1);

        let display = dict.get_diplay_entry(1);

        // First span covers "hok"
        let (start1, end1) = spans[0];
        assert_eq!(&display.jyutping[start1..end1], "hok6");
    }

    #[test]
    fn test_jyutping_matched_spans_partial_with_tone() {
        let dict = create_test_dict();
        // Use entry 1 which has "hok6 saang1"
        // We'll create a custom entry that has "hon5" to better test the case
        // Actually, let's use "saang1" and query "saa1" which we already have
        // Or better, let's conceptually test with what we have
        // Query "saa1" against "saang1" should highlight "saa" and "1" separately
        let entry = &dict.entries[1];

        let query_terms = QueryTerms {
            jyutping_terms: vec![JyutpingQueryTerm::create("saa1", &dict.jyutping_store)],
            traditional_terms: vec![],
        };

        let spans = dict.get_jyutping_matched_spans(entry, &query_terms);

        // Should have 2 spans for the partial match with tone
        assert_eq!(spans.len(), 2, "Partial match with tone should produce 2 spans");

        let display = dict.get_diplay_entry(1);

        // First span should cover only the matched portion "saa"
        let (start1, end1) = spans[0];
        let matched_base = &display.jyutping[start1..end1];
        assert_eq!(matched_base, "saa", "First span should highlight matched base 'saa'");

        // Second span should cover the tone "1"
        let (start2, end2) = spans[1];
        let matched_tone = &display.jyutping[start2..end2];
        assert_eq!(matched_tone, "1", "Second span should highlight tone digit '1'");

        // Verify the unmatched portion "ng" is between the two spans
        assert!(end1 < start2, "There should be a gap between base and tone spans");
        let gap = &display.jyutping[end1..start2];
        assert_eq!(gap, "ng", "Gap between spans should be the unmatched 'ng'");
    }

    #[test]
    fn test_jyutping_matched_spans_substring_with_tone() {
        let dict = create_test_dict();
        let entry = &dict.entries[1];

        // Query with substring and tone should still work
        let query_terms = QueryTerms {
            jyutping_terms: vec![JyutpingQueryTerm::create("saa1", &dict.jyutping_store)],
            traditional_terms: vec![],
        };

        let spans = dict.get_jyutping_matched_spans(entry, &query_terms);

        // Should have two spans: one for "saa", one for "1"
        assert_eq!(spans.len(), 2);

        let display = dict.get_diplay_entry(1);

        // First span covers "saa" (the matched base portion)
        let (start1, end1) = spans[0];
        assert_eq!(&display.jyutping[start1..end1], "saa");

        // Second span covers "1" (the tone digit)
        let (start2, end2) = spans[1];
        assert_eq!(&display.jyutping[start2..end2], "1");
    }

    #[test]
    fn test_jyutping_substring_highlighting_integration() {
        let dict = create_test_dict();

        // Search with substring
        let results = dict.search("saa", 8, Box::new(TestStopwatch)).matches;

        assert!(results.len() > 0, "Should find results for substring");
        assert_eq!(results[0].match_obj.entry_id, 1);
        assert!(matches!(results[0].match_obj.match_type, MatchType::Jyutping));

        // Verify the highlighting span is correct
        assert_eq!(results[0].matched_spans.len(), 1);
        let (start, end) = results[0].matched_spans[0];


        let display = dict.get_diplay_entry(1);
        let highlighted = &display.jyutping[start..end];

        // The highlighted portion should be only the typed substring "saa"
        assert_eq!(highlighted, "saa",
            "Substring search 'saa' should highlight only typed 'saa'");
    }

    #[test]
    fn test_jyutping_prefix_highlighting_integration() {
        let dict = create_test_dict();

        // Search with prefix
        let results = dict.search("ho", 8, Box::new(TestStopwatch)).matches;

        assert!(results.len() > 0, "Should find results for prefix");
        assert_eq!(results[0].match_obj.entry_id, 1);

        // Verify the highlighting span covers the full syllable
        let (start, end) = results[0].matched_spans[0];

        let display = dict.get_diplay_entry(1);
        let highlighted = &display.jyutping[start..end];

        assert_eq!(highlighted, "ho",
            "Prefix search 'ho' should highlight only typed 'ho'");
    }

    #[test]
    fn test_jyutping_multiple_substring_matches() {
        let dict = create_test_dict();

        // Search with two substring queries
        let results = dict.search("ho saa", 8, Box::new(TestStopwatch)).matches;

        assert!(results.len() > 0);
        assert_eq!(results[0].match_obj.entry_id, 1); // 學生 with "hok6 saang1"

        // Should have two highlighted spans
        assert_eq!(results[0].matched_spans.len(), 2);

        let display = dict.get_diplay_entry(1);

        // First span should highlight only typed "ho"
        let (start1, end1) = results[0].matched_spans[0];
        assert_eq!(&display.jyutping[start1..end1], "ho");

        // Second span should highlight only typed "saa"
        let (start2, end2) = results[0].matched_spans[1];
        assert_eq!(&display.jyutping[start2..end2], "saa");
    }

    #[test]
    fn test_english_matched_spans() {
        let dict = create_test_dict();
        let entry = &dict.entries[0];

        let spans = dict.get_english_matched_spans(entry, "teach");

        assert_eq!(spans.len(), 1);
        let (start, end) = spans[0];

        let display = dict.get_diplay_entry(0);
        let matched_text = &display.english_definitions[0][start..end];

        assert_eq!(matched_text, "teach");
    }

    #[test]
    fn test_traditional_matched_spans() {
        let dict = create_test_dict();
        let entry = &dict.entries[0];

        let query_terms = QueryTerms {
            jyutping_terms: vec![],
            traditional_terms: vec![3], // 老 is at index 3 in sorted character_store
        };

        let spans = dict.get_traditional_matched_spans(entry, &query_terms);

        assert_eq!(spans.len(), 1);
        let (start, end) = spans[0];

        assert_eq!(start, 0); // First character in entry
        assert_eq!(end, 1);

        let display = dict.get_diplay_entry(0);
        let chars: Vec<char> = display.characters.chars().collect();
        assert_eq!(chars[start], '老');
    }

    #[test]
    fn test_integration_jyutping_search() {
        let dict = create_test_dict();

        // Search for "lou" should return the teacher entry
        let results = dict.search("lou", 8, Box::new(TestStopwatch)).matches;

        assert!(results.len() > 0, "Should find at least one result");
        assert_eq!(results[0].match_obj.entry_id, 0);
        assert!(matches!(results[0].match_obj.match_type, MatchType::Jyutping));

        // Verify the spans are valid
        let display = dict.get_diplay_entry(results[0].match_obj.entry_id);
        // For Jyutping matches, spans refer to positions in the jyutping string
        for (start, end) in &results[0].matched_spans {
            assert!(*start < display.jyutping.len());
            assert!(*end <= display.jyutping.len());
            let matched = &display.jyutping[*start..*end];
            assert!(matched.len() > 0, "Matched span should not be empty");
        }
    }

    #[test]
    fn test_integration_english_search() {
        let dict = create_test_dict();

        let results = dict.search("student", 8, Box::new(TestStopwatch)).matches;

        assert!(results.len() > 0);
        assert_eq!(results[0].match_obj.entry_id, 1);
        assert!(matches!(results[0].match_obj.match_type, MatchType::English));

        // Verify english spans - spans now refer to positions within the concatenated english data
        // For English matches, we just verify the spans are valid within the english data
        assert!(results[0].matched_spans.len() > 0, "Should have at least one matched span");
    }

    #[test]
    fn test_integration_traditional_search() {
        let dict = create_test_dict();

        // First verify the character is in the store
        let char_id = dict.character_store.char_to_index('老');
        eprintln!("Character '老' has id: {:?}", char_id);
        eprintln!("Entry 0 characters: {:?}", dict.entries[0].characters);

        let results = dict.search("老", 8, Box::new(TestStopwatch)).matches;

        // Debug: print what we got
        eprintln!("Search for '老' returned {} results", results.len());
        for (i, r) in results.iter().enumerate() {
            eprintln!("Result {}: entry_id={}, match_type={:?}, spans={:?}",
                i, r.match_obj.entry_id, r.match_obj.match_type, r.matched_spans);
        }

        assert!(results.len() > 0, "Should find at least one result for '老'");
        assert_eq!(results[0].match_obj.entry_id, 0);
        assert!(matches!(results[0].match_obj.match_type, MatchType::Traditional));

        // Verify traditional spans point to the character
        let display = dict.get_diplay_entry(results[0].match_obj.entry_id);
        // For Traditional matches, spans refer to character positions
        for (start, end) in &results[0].matched_spans {
            let chars: Vec<char> = display.characters.chars().collect();
            assert!(*start < chars.len());
            assert!(*end <= chars.len());
            assert_eq!(chars[*start], '老');
        }
    }

    #[test]
    pub fn test_aa_baa() {
        // Create a minimal test dictionary
        // Characters must be in sorted order!
        let character_store = CharacterStore {
            characters: vec!['學', '師', '生', '老'], // sorted order
        };

        let jyutping_store = JyutpingStore {
            base_strings: vec![
                "aa".to_string(),
                "baa".to_string(),
            ],
        };

        let entries = vec![
            CompiledDictionaryEntry {
                characters: vec![0, 1],
                jyutping: vec![
                    Jyutping { base: 0, tone: 3 },
                    Jyutping { base: 1, tone: 1 },
                ],
                english_start: 0,
                english_end: 1,
                cost: 0,
                flags: FLAG_SOURCE_CEDICT,
            }
        ];

        let english_data = b"father".to_vec();
        let english_data_starts = vec![0, 6];

        let dict = CompiledDictionary {
            character_store,
            jyutping_store,
            entries,
            english_data,
            english_data_starts,
        };

        // Should be exact match
        let res = dict.search("aa baa", 8, Box::new(TestStopwatch));
        assert_eq!(1, res.matches.len());
        assert_eq!(0, res.matches[0].match_obj.cost_info.total());
        assert_eq!(2, res.matches[0].matched_spans.len());
        assert_eq!((0, 2), res.matches[0].matched_spans[0]);
        assert_eq!((4, 7), res.matches[0].matched_spans[1]);

        let res = dict.search("aa ba", 8, Box::new(TestStopwatch));
        assert_eq!(1, res.matches.len());
        assert_eq!(2500, res.matches[0].match_obj.cost_info.total());
        assert_eq!(2, res.matches[0].matched_spans.len());
        assert_eq!((0, 2), res.matches[0].matched_spans[0]);
        assert_eq!((4, 6), res.matches[0].matched_spans[1]);

        // Should be exact match
        let res = dict.search("aa3 baa1", 8, Box::new(TestStopwatch));
        assert_eq!(1, res.matches.len());
        assert_eq!(0, res.matches[0].match_obj.cost_info.total());
        assert_eq!(1, res.matches[0].matched_spans.len());
        assert_eq!((0, 8), res.matches[0].matched_spans[0]);
    }
}
