use std::collections::{HashSet, BTreeSet};

use bit_set::BitSet;

use crate::{Dictionary, jyutping_splitter::JyutpingSplitter, data_writer::DataWriter};

pub struct CompiledDictionary
{
    character_store : CharacterStore,
    jyutping_store : JyutpingStore,

    entries : Vec<DictionaryEntry>,
}

pub const FILE_HEADER: &[u8] = b"jyp_dict";
pub const CURRENT_VERSION: u32 = 1;

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

        let mut all_characters_list : Vec<char> = all_characters.into_iter().collect();
        let mut all_jyutping_words_list : Vec<String> = all_jyutping_words.into_iter().collect();

        let character_store = CharacterStore::from_chars(all_characters_list);
        let jyutping_store = JyutpingStore::from_strings(all_jyutping_words_list);

        println!("Individual characters {}, Individual jyutping words {}", character_store.characters.len(), jyutping_store.base_strings.len());
        //println!("{:#?}", character_store.characters);
        //println!("{:#?}", jyutping_store.base_strings);

        let mut entries = Vec::new();

        for (traditional_chars, definitions) in dict.trad_to_def.inner
        {
            let mut cost : f32 = 0.0;

            let mut char_indexes = Vec::new();

            for character in traditional_chars.chars()
            {
                char_indexes.push(character_store.char_to_index(character).expect(&format!("Could not find match for {}", character)));

                cost += dict.trad_to_frequency.get_or_default(character).cost as f32;
            }

            let mut jyutpings = Vec::new();
            if let Some(jyutping_string) = dict.trad_to_jyutping.inner.get(&traditional_chars)
            {
                for word in JyutpingSplitter::new(jyutping_string)
                {
                    jyutpings.push(jyutping_store.get(word).unwrap());
                }
            }

            entries.push(DictionaryEntry {
                characters: char_indexes,
                jyutpings: jyutpings,
                english_definitions: definitions,
                cost,
            });
        }

        entries.sort_by(|x, y| x.cost.total_cmp(&y.cost));

        Self {
            character_store,
            jyutping_store,
            entries,
        }
    }
}

impl CompiledDictionary {
    pub fn search(&self, s : &str) -> Vec<&DictionaryEntry>
    {
        let mut jyutping_matches = Vec::new();
        for query_term in s.split_ascii_whitespace()
        {
            jyutping_matches.push(self.get_jyutping_matches(query_term));
        }

        let mut matches = Vec::new();

        let max = 10;
        for x in &self.entries
        {
            if (self.matches_query(x, &jyutping_matches))
            {
                matches.push(x);

                if (matches.len() >= max)
                {
                    break;
                }
            }
        }

        matches
    }

    pub fn matches_query(&self, entry: &DictionaryEntry, query_terms : &[(BitSet, Option<u8>)]) -> bool
    {
        for (bitset, tone) in query_terms
        {
            let mut term_match = false;

            for j in &entry.jyutpings
            {
                if (bitset.contains(j.base as usize))
                {
                    if let Some(t) = tone
                    {
                        if *t == j.tone {
                            term_match = true;
                            break;
                        }
                    }
                    else
                    {
                        term_match = true;
                        break;
                    }
                }
            }

            if (!term_match)
            {
                return false;
            }
        }

        true
    }

    pub fn get_jyutping_matches(&self, mut s : &str) -> (BitSet, Option<u8>)
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

        //for (i, jyutping) in self.jyutping_store.base_strings
        //    for jyutping_word in jyutping {
        //        if self.jyutping_store.matches(*jyutping_word, s, None) {
        //            matches.insert(i);
        //        }
        //    }
        //}

        for (i, jyutping_string) in self.jyutping_store.base_strings.iter().enumerate()
        {
            if (jyutping_string.contains(s))
            {
                println!("'{}' matches {}", s, jyutping_string);
                matches.insert(i);
            }
        }

        (matches, tone)
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
            writer.write_u8(e.characters.len() as u8)?;
            for j in &e.jyutpings
            {
                writer.write_vbyte(j.base as u64)?;
                writer.write_u8(j.tone)?;
            }

            assert!(e.english_definitions.len() < 256);
            writer.write_u8(e.english_definitions.len() as u8)?;
            for def in &e.english_definitions
            {
                // Some dummy offset
                writer.write_u32(100);
                //writer.write_string(def)?;
            }
        }

        Ok(())
    }
}

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
struct Jyutping
{
    // TODO merge to single u16
    base : u16,
    tone : u8,
}


#[derive(Debug, Clone)]
pub struct DictionaryEntry
{
    characters : Vec<u16>,
    // TODO struct of array members here
    jyutpings : Vec<Jyutping>,
    english_definitions : Vec<String>,
    cost : f32,
}

#[derive(Debug)]
pub struct DisplayDictionaryEntry
{
    characters : String,
    jyutping : String,
    english_definitions : Vec<String>,
    cost : f32,
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

        Self {
            characters,
            jyutping,
            english_definitions: entry.english_definitions.clone(),
            cost : entry.cost,
        }
    }
}