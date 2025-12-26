use crate::compiled_dictionary::*;
use crate::search::*;

impl CompiledDictionary {
    pub fn get_jyutping_matched_spans(&self, entry: &CompiledDictionaryEntry, query_terms: &QueryTerms) -> Vec<(usize, usize)> {
        let mut spans = Vec::new();

        // What do we want
        // We have a series of query terms: q0, q1, ... qn
        // And a series of entry terms: e0, e1, ... em
        // We want to find the "best" map from query terms into entry terms
        // ie the mapping that minimises some cost function.
        //
        // Once we have that we can determine the matched spans

        struct Mapping {
            inner: Vec<(u16, u16)>,
        }

        let mut mappings : Vec<(Mapping, u32)> = Vec::new();

        // N^2 how bad is that?

        for (jyutping_id, entry_jyutping) in entry.jyutping.iter().enumerate() {
            for query_jyutping in &query_terms.jyutping_terms {
                if query_jyutping.matches.contains(entry_jyutping.base as usize) {
                }
            }
        }

        spans
    }

    pub fn get_english_matched_spans(&self, entry: &CompiledDictionaryEntry, query: &str) -> Vec<(usize, usize)> {
        let mut spans = Vec::new();

        if entry.english_start == entry.english_end {
            return spans;
        }

        //let first_start = self.english_data_starts[entry.english_start as usize] as usize;

        for def_idx in entry.english_start..entry.english_end {
            let start = self.english_data_starts[def_idx as usize] as usize;
            let end = if def_idx + 1 < self.english_data_starts.len() as u32 {
                self.english_data_starts[def_idx as usize + 1] as usize
            } else {
                self.english_data.len()
            };
            let def_bytes = &self.english_data[start..end];

            for split in query.split_ascii_whitespace() {
                if let Some(pos) = crate::string_search::string_indexof_linear_ignorecase(split, def_bytes) {
                    //let field_idx = 2 + (def_idx - entry.english_start) as usize;
                    //let pos = pos + first_start;
                    spans.push((start + pos, start + pos + split.len()));
                }
            }
        }

        spans
    }

    pub fn get_traditional_matched_spans(&self, entry: &CompiledDictionaryEntry, query_terms: &QueryTerms) -> Vec<(usize, usize)> {
        let mut spans = Vec::new();

        for (char_idx, char_id) in entry.characters.iter().enumerate() {
            if query_terms.traditional_terms.contains(char_id) {
                spans.push((char_idx, char_idx + 1));
            }
        }

        spans
    }
}
