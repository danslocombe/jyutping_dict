use std::collections::{HashSet, BTreeSet};

use bit_set::BitSet;

use crate::{Dictionary, jyutping_splitter::JyutpingSplitter};

pub struct CompiledDictionary
{
    character_store : CharacterStore,
    jyutping_store : JyutpingStore,

    entries : Vec<DictionaryEntry>,
}

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
            let mut score = 0.0;

            let mut char_indexes = Vec::new();

            for character in traditional_chars.chars()
            {
                char_indexes.push(character_store.char_to_index(character).expect(&format!("Could not find match for {}", character)));
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
                cost: 0,
            });
        }

        Self {
            character_store,
            jyutping_store,
            entries,
        }
    }
}

impl CompiledDictionary {
    pub fn search_single(&self, s : &str) -> Vec<&DictionaryEntry>
    {
        let bitset = self.get_jyutping_matches(s);

        let mut matches = Vec::new();

        let max = 10;
        for x in &self.entries
        {
            if (x.jyutpings.len() == 0)
            {
                continue;
            }

            let mut is_match = true;
            for j in &x.jyutpings
            {
                if (!bitset.contains(j.base as usize))
                {
                    is_match = false;
                    break;
                }
            }

            if (is_match)
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

    pub fn get_jyutping_matches(&self, s : &str) -> BitSet
    {
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
                println!("{} match {}", s, jyutping_string);
                matches.insert(i);
            }
        }

        matches
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
    cost : i32,
}