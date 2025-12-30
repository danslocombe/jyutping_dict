use core::str;
use std::cell::RefCell;

use bit_set::BitSet;
use serde::Serialize;

use crate::Stopwatch;

use crate::compiled_dictionary::*;
use crate::reconstruct_match::merge_overlapping_match_spans;

pub const OUT_OF_ORDER_INVERSION_PENALTY: u32 = 8_000;
pub const UNMATCHED_JYUTPING_PENALTY: u32 = 10_000;
pub const JYUTPING_PARTIAL_MATCH_PENALTY_K : u32 = 12_000;
pub const JYUTPING_COMPLETION_PENALTY_K : u32 = 2_500;
pub const JYUTPING_PREFIX_LEVENSHTEIN_PENALTY_K: u32 = 20_000;

pub const ENGLISH_BASE_PENALTY: u32 = 5_000;
pub const NON_ASCII_MATCH_IN_ENGLISH_PENALTY: u32 = 8_000;
pub const ENGLISH_POS_OFFSET_PENALTY_K: u32 = 100;
pub const ENGLISH_MIDDLE_OF_WORD_PENALTY: u32 = 5_000;


pub struct QueryTerms {
    pub jyutping_terms: Vec<JyutpingQueryTerm>,
    pub traditional_terms: Vec<u16>,
}

pub struct JyutpingQueryTerm {
    pub string_no_tone : String,
    pub tone: Option<u8>,

    pub matches: BitSet,
    pub match_bit_to_match_cost: Vec<(i32, u32)>,
}

impl JyutpingQueryTerm {
    pub fn create(s : &str, jyutping_store: &JyutpingStore) -> Self
    {
        debug_assert!(s.len() > 0);

        let (s, tone) = crate::jyutping_splitter::parse_jyutping_tone(s);

        let mut matches = BitSet::new();
        let mut match_bit_to_match_cost = Vec::new();

        debug_assert!(jyutping_store.base_strings.len() < std::i32::MAX as usize);

        for (i, jyutping_string) in jyutping_store.base_strings.iter().enumerate()
        {
            if (jyutping_string.eq_ignore_ascii_case(s))
            {
                matches.insert(i);
                continue;
            }

            if let Some(idx) = crate::string_search::string_indexof_linear_ignorecase(s, jyutping_string.as_bytes())
            {
                let mut match_cost = idx as u32 * JYUTPING_PARTIAL_MATCH_PENALTY_K;
                match_cost += (jyutping_string.len() - s.len()) as u32 * JYUTPING_COMPLETION_PENALTY_K;
                match_bit_to_match_cost.push((i as i32, match_cost));
                matches.insert(i);
                continue;
            }

            // Warning: Noisy
            let dist = crate::string_search::prefix_levenshtein_ascii(s, jyutping_string);
            if (dist < 2) {
                let match_cost = dist as u32 * JYUTPING_PREFIX_LEVENSHTEIN_PENALTY_K;
                match_bit_to_match_cost.push((i as i32, match_cost));
                matches.insert(i);
                continue;
            }
        }

        Self {
            string_no_tone: s.to_owned(),
            tone,
            matches,
            match_bit_to_match_cost,
        }
    }

    pub fn string_with_tone(&self) -> String
    {
        let mut string = String::with_capacity(self.string_no_tone.len() + if self.tone.is_some() { 1 } else { 0 });
        string.push_str(&self.string_no_tone);
        if let Some(t) = self.tone {
            string.push((t + b'0') as char);
        }

        string
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
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

#[derive(Debug, Clone, Copy, Serialize)]
pub enum MatchType {
    Jyutping,
    Traditional,
    English,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Match
{
    pub cost_info : MatchCostInfo,
    pub match_type: MatchType,
    pub entry_id: usize,
}

#[derive(Debug, Serialize)]
pub struct MatchWithHitInfo {
    pub match_obj: Match,
    pub matched_spans: Vec<(usize, usize)>,
}

#[derive(Debug, Default, Serialize)]
pub struct Timings {
    pub jyutping_pre_ms: i32,
    pub traditional_pre_ms: i32,

    pub full_match: i32,
    pub rank: i32,
}

#[derive(Debug, Default, Serialize)]
pub struct SearchResult {
    pub matches : Vec<MatchWithHitInfo>,
    pub timings: Timings,
    pub internal_candidates: usize,
}

impl CompiledDictionary {
    pub fn search(&self, s : &str, max_results: usize, stopwatch: Box<dyn Stopwatch>) -> SearchResult
    {
        let mut result = SearchResult::default();

        let mut jyutping_query_terms = Vec::new();
        for query_term in s.split_ascii_whitespace()
        {
            jyutping_query_terms.push(JyutpingQueryTerm::create(query_term, &self.jyutping_store));
        }

        result.timings.jyutping_pre_ms = stopwatch.elapsed_ms();

        let mut traditional_terms = Vec::new();
        for c in s.chars()
        {
            if let Some(c_id) = self.character_store.char_to_index(c) {
                traditional_terms.push(c_id);
            }
        }

        result.timings.traditional_pre_ms = stopwatch.elapsed_ms();

        let query_terms = QueryTerms {
            jyutping_terms: jyutping_query_terms,
            traditional_terms,
        };

        let mut matches: Vec<Match> = Vec::new();

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
            }
            else
            {
                let force_english = false;
                if (s.len() > 2 || force_english)
                {
                    if let Some(cost_info) = self.matches_query_english(x, s)
                    {
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

        result.timings.full_match = stopwatch.elapsed_ms();

        result.internal_candidates = matches.len();
        debug_log!("Internal candidates: {}", result.internal_candidates);

        matches.sort_by(|(x), (y)| x.cost_info.total().cmp(&y.cost_info.total()));
        matches.truncate(max_results);

        result.timings.rank = stopwatch.elapsed_ms();

        let mut matches_with_hit_info = Vec::with_capacity(matches.len());
        for m in matches
        {
            let entry = &self.entries[m.entry_id];
            let mut matched_spans = match m.match_type {
                MatchType::Jyutping => self.get_jyutping_matched_spans(entry, &query_terms),
                MatchType::Traditional => self.get_traditional_matched_spans(entry, &query_terms),
                MatchType::English => self.get_english_matched_spans(entry, s),
            };

            matched_spans = merge_overlapping_match_spans(matched_spans);

            matches_with_hit_info.push(MatchWithHitInfo {
                match_obj: m,
                matched_spans,
            })
        }

        result.matches = matches_with_hit_info;

        result
    }
}

#[thread_local]
static mut  s_entry_jyutping_matches : Option<BitSet> = None;

#[thread_local]
static mut s_matched_positions : Option<Vec<usize>> = None;

impl CompiledDictionary {
    pub fn matches_jyutping_term(&self, entry: &CompiledDictionaryEntry, query_terms : &QueryTerms) -> Option<MatchCostInfo> {
        // If no jyutping terms in query, this is not a jyutping match
        if query_terms.jyutping_terms.is_empty() {
            return None;
        }

        if (entry.jyutping.len() < query_terms.jyutping_terms.len()) {
            return None;
        }

        let mut total_term_match_cost = 0;


        // We are storing this in a thread_local to try and avoid dynamic
        // allocations as much as possible.
        unsafe {
            if (s_entry_jyutping_matches.is_none()) {
                s_entry_jyutping_matches = Some(BitSet::new());
            }
            if (s_matched_positions.is_none()) {
                s_matched_positions = Some(Vec::with_capacity(1024));
            }
        }

        let entry_jyutping_matches = unsafe { s_entry_jyutping_matches.as_mut().unwrap() };
        let matched_positions = unsafe { s_matched_positions.as_mut().unwrap() };
        entry_jyutping_matches.clear();
        matched_positions.clear();

        for jyutping_term in &query_terms.jyutping_terms
        {
            let mut best_term_match: Option<(usize, u32)> = None;

            for (i, entry_jyutping) in entry.jyutping.iter().enumerate()
            {
                if (jyutping_term.matches.contains(entry_jyutping.base as usize))
                {
                    let mut term_match_cost = 0;
                    for (match_bit, cost) in &jyutping_term.match_bit_to_match_cost {
                        if (*match_bit == entry_jyutping.base as i32) {
                            term_match_cost = *cost;
                            // Break out of finding term_match_cost.
                            break;
                        }
                    }

                    let mut term_match = false;
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
                        let mut should_update = best_term_match.is_none();
                        if let Some((_, existing_best_cost)) = best_term_match {
                            should_update = term_match_cost < existing_best_cost;
                        }

                        if (should_update) {
                            best_term_match = Some((i, term_match_cost));
                        }
                    }
                }
            }

            if let Some((best_match, best_cost)) = best_term_match
            {
                total_term_match_cost += best_cost;
                entry_jyutping_matches.insert(best_match);
                matched_positions.push(best_match);
            }
            else
            {
                return None;
            }
        }

        //let additional_terms = entry.jyutpings.len() - query_terms.jyutping_matches.len();
        //match_cost += additional_terms as u32 * 10_000;

        let inversion_cost = cost_inversions(&matched_positions);

        let mut unmatched_position_cost = 0u32;
        for i in 0..entry.jyutping.len() {
            if (!entry_jyutping_matches.contains(i)) {
                unmatched_position_cost += ((entry.jyutping.len() + 1) - i) as u32 * UNMATCHED_JYUTPING_PENALTY;
            }
        }

        Some(MatchCostInfo {
            term_match_cost: total_term_match_cost,
            unmatched_position_cost,
            inversion_cost,
            static_cost: 0,
        })
    }
}

impl CompiledDictionary {
    pub fn matches_query_english(&self, entry: &CompiledDictionaryEntry, s : &str) -> Option<MatchCostInfo>
    {
        // Make sure we prefer jyutping matches
        let mut match_cost: u32 = ENGLISH_BASE_PENALTY;

        // @Perf do search over enterity instead of individual entries.

        if (entry.english_start == entry.english_end)
        {
            return None;
        }

        let start = self.english_data_starts[entry.english_start as usize] as usize;
        let end = self.english_data_starts[entry.english_end as usize] as usize;
        let block = &self.english_data[start..end];

        // We are storing this in a thread_local to try and avoid dynamic
        // allocations as much as possible.
        unsafe {
            if (s_matched_positions.is_none()) {
                s_matched_positions = Some(Vec::with_capacity(1024));
            }
        }

        let matched_positions = unsafe { s_matched_positions.as_mut().unwrap() };
        matched_positions.clear();

        for split in s.split_ascii_whitespace()
        {
            /*
            for (i, def) in entry.english_definitions.iter().enumerate()
            {
                if let Some(pos) = crate::string_search::string_indexof_linear_ignorecase(split, def) {
                    match_cost += i as u32 * 1_000;
                    match_cost += pos as u32 * 100;
                    continue 'outer;
                }
            }
            */

            // @FIXME entry boundaries etc.

            if let Some(pos) = crate::string_search::string_indexof_linear_ignorecase(split, block) {
                matched_positions.push(pos);
                match_cost += pos as u32 * ENGLISH_POS_OFFSET_PENALTY_K;

                if (pos == 0)  {
                    continue;
                }

                let start_c = block[pos-1];
                if (start_c.is_ascii_whitespace() || start_c == b'-')
                {
                    continue;
                }

                // Match in the middle of a word
                match_cost += ENGLISH_MIDDLE_OF_WORD_PENALTY;
                continue;
            }

            // No match on this split
            return None;
        }

        let inversion_cost = cost_inversions(&matched_positions);

        for c in s.chars() {
            if (!c.is_ascii()) {
                // Non-ascii match, probably a chinese character
                // match within an english description
                match_cost += NON_ASCII_MATCH_IN_ENGLISH_PENALTY;
            }
        }


        Some(MatchCostInfo {
            term_match_cost: match_cost,
            unmatched_position_cost: 0,
            inversion_cost,
            static_cost: entry.cost,
        })
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
}


pub fn cost_inversions(matched_positions: &[usize]) -> u32
{
    let mut cost = 0u32;
    for i in 0..matched_positions.len() {
        for j in (i + 1)..matched_positions.len() {
            if matched_positions[i] > matched_positions[j] {
                cost += OUT_OF_ORDER_INVERSION_PENALTY;
            }
        }
    }

    cost
}
