use serde::Serialize;
use crate::compiled_dictionary::{CompiledDictionary, CompiledDictionaryEntry, Match, MatchType, MatchWithHitInfo};
use crate::EntrySource;

/// A dictionary entry with pre-rendered and highlighted fields ready for display
#[derive(Debug, Serialize)]
pub struct RenderedResult {
    pub characters: String,
    pub jyutping: String,
    pub english_definitions: Vec<String>,
    pub cost: u32,
    pub entry_source: EntrySource,
}

impl RenderedResult {
    /// Create a rendered result from a match, with hit highlighting applied
    pub fn from_match(match_result: &MatchWithHitInfo, dict: &CompiledDictionary) -> Self {
        let entry = &dict.entries[match_result.match_obj.entry_id];

        let mut characters =
        {
            let mut characters = String::new();
            for c in &entry.characters {
                characters.push(dict.character_store.characters[*c as usize]);
            }
            characters
        };

        if let MatchType::Traditional = match_result.match_obj.match_type {
            // @AI Need to audit
            // Traditional matched spans are character indices, not byte indices
            // Convert them to byte indices for highlighting
            let byte_spans: Vec<(usize, usize)> = match_result.matched_spans.iter()
                .map(|&(char_start, char_end)| {
                    let mut byte_start = 0;
                    let mut byte_end = 0;
                    for (idx, (byte_idx, _)) in characters.char_indices().enumerate() {
                        if idx == char_start {
                            byte_start = byte_idx;
                        }
                        if idx == char_end {
                            byte_end = byte_idx;
                            break;
                        }
                    }
                    // If char_end is past the last character, set byte_end to the end of the string
                    if char_end >= characters.chars().count() {
                        byte_end = characters.len();
                    }
                    (byte_start, byte_end)
                })
                .collect();
            characters = Self::apply_highlights(&characters, &byte_spans);
        }

        let mut jyutping =
        {
            let mut jyutping = String::new();
            for j in &entry.jyutping {
                if !jyutping.is_empty() {
                    jyutping.push(' ');
                }
                jyutping.push_str(&dict.jyutping_store.base_strings[j.base as usize]);
                jyutping.push((j.tone + b'0') as char);
            }
            jyutping
        };

        if let MatchType::Jyutping = match_result.match_obj.match_type {
            jyutping = Self::apply_highlights(&jyutping, &match_result.matched_spans);
        }

        let english_definitions = if let MatchType::English = match_result.match_obj.match_type {
            Self::build_english_definitions_with_highlights(entry, dict, &match_result.matched_spans)
        } else {
            Self::build_english_definitions_with_highlights(entry, dict, &[])
        };

        Self {
            characters,
            jyutping,
            english_definitions,
            cost: entry.cost,
            entry_source: entry.get_source(),
        }
    }

    fn build_english_definitions_with_highlights(
        entry: &CompiledDictionaryEntry,
        dict: &CompiledDictionary,
        matched_spans: &[(usize, usize)]
    ) -> Vec<String> {
        let mut english_definitions = Vec::with_capacity(
            entry.english_end as usize - entry.english_start as usize
        );

        for i in entry.english_start..entry.english_end {
            let start = dict.english_data_starts[i as usize] as usize;
            let end = dict.english_data_starts[i as usize + 1] as usize;
            let blob = &dict.english_data[start..end];
            let plain_text = unsafe { std::str::from_utf8_unchecked(blob) };

            let mut filtered_modified_matches = Vec::with_capacity(matched_spans.len());
            for &(span_start_abs, span_end_abs) in matched_spans {
                if (span_start_abs < start)
                {
                    continue;
                }

                if (span_end_abs > end)
                {
                    continue;
                }

                filtered_modified_matches.push((span_start_abs - start, span_end_abs - start));
            }

            let highlighted = Self::apply_highlights(plain_text, &filtered_modified_matches);
            english_definitions.push(highlighted);
        }

        english_definitions
    }

    /// Apply HTML highlighting markup to text based on matched spans
    /// Spans are (start, end) positions in the text
    fn apply_highlights(text: &str, matched_spans: &[(usize, usize)]) -> String {
        if matched_spans.is_empty() {
            return Self::escape_html(text);
        }

        debug_assert!(matched_spans.is_sorted_by_key(|(x, _)| x));

        let mut result = String::new();
        let mut last_pos = 0;

        for &(start, end) in matched_spans {
            // Add text before the match
            if start > last_pos {
                result.push_str(&Self::escape_html(&text[last_pos..start]));
            }

            // Add highlighted match
            result.push_str("<mark class=\"hit-highlight\">");
            result.push_str(&Self::escape_html(&text[start..end]));
            result.push_str("</mark>");

            last_pos = end;
        }

        // Add remaining text
        if last_pos < text.len() {
            result.push_str(&Self::escape_html(&text[last_pos..]));
        }

        result
    }

    /// Escape HTML special characters to prevent XSS
    fn escape_html(text: &str) -> String {
        text.chars()
            .map(|c| match c {
                '<' => "&lt;".to_string(),
                '>' => "&gt;".to_string(),
                '&' => "&amp;".to_string(),
                '"' => "&quot;".to_string(),
                '\'' => "&#x27;".to_string(),
                _ => c.to_string(),
            })
            .collect()
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiled_dictionary::tests::create_test_dict;

    struct TestStopwatch;

    impl crate::Stopwatch for TestStopwatch {
        fn elapsed_ms(&self) -> i32 {
            0
        }
    }

    #[test]
    fn test_escape_html() {
        assert_eq!(
            RenderedResult::escape_html("<script>alert('xss')</script>"),
            "&lt;script&gt;alert(&#x27;xss&#x27;)&lt;/script&gt;"
        );

        assert_eq!(
            RenderedResult::escape_html("a & b"),
            "a &amp; b"
        );

        assert_eq!(
            RenderedResult::escape_html("normal text"),
            "normal text"
        );
    }

    #[test]
    fn test_apply_highlights_single_span() {
        let text = "hello world";
        let spans = vec![(0, 5)];
        let result = RenderedResult::apply_highlights(text, &spans);
        assert_eq!(result, "<mark class=\"hit-highlight\">hello</mark> world");
    }

    #[test]
    fn test_apply_highlights_multiple_spans() {
        let text = "hello world";
        let spans = vec![(0, 5), (6, 11)];
        let result = RenderedResult::apply_highlights(text, &spans);
        assert_eq!(
            result,
            "<mark class=\"hit-highlight\">hello</mark> <mark class=\"hit-highlight\">world</mark>"
        );
    }

    #[test]
    fn test_apply_highlights_no_spans() {
        let text = "hello world";
        let spans = vec![];
        let result = RenderedResult::apply_highlights(text, &spans);
        assert_eq!(result, "hello world");
    }

    #[test]
    fn test_apply_highlights_with_html_chars() {
        let text = "<tag> & more";
        let spans = vec![(0, 5)];
        let result = RenderedResult::apply_highlights(text, &spans);
        assert_eq!(
            result,
            "<mark class=\"hit-highlight\">&lt;tag&gt;</mark> &amp; more"
        );
    }

    #[test]
    fn test_from_match_jyutping_highlighting() {
        let dict = create_test_dict();

        // Search for "lou" which should match 老師 (lou5 si1)
        let results = dict.search("lou", Box::new(TestStopwatch)).matches;
        assert!(results.len() > 0);

        let result = &results[0];
        assert!(matches!(result.match_type, MatchType::Jyutping));

        // Create rendered result with highlighting
        let rendered = RenderedResult::from_match(result, &dict);

        // Check that jyutping has highlighting markup
        assert!(rendered.jyutping.contains("<mark class=\"hit-highlight\">"));
        assert!(rendered.jyutping.contains("lou</mark>"));

        // Characters should not have highlighting (only jyutping matches)
        assert!(!rendered.characters.contains("<mark"));

        // English should not have highlighting
        for def in &rendered.english_definitions {
            assert!(!def.contains("<mark"));
        }
    }

    #[test]
    fn test_from_match_jyutping_with_tone() {
        let dict = create_test_dict();

        // Search for "lou5" which should match 老師 (lou5 si1)
        let results = dict.search("lou5", Box::new(TestStopwatch)).matches;
        assert!(results.len() > 0);

        let result = &results[0];
        let rendered = RenderedResult::from_match(result, &dict);

        // Should highlight both base and tone
        assert!(rendered.jyutping.contains("<mark class=\"hit-highlight\">lou</mark>"));
        assert!(rendered.jyutping.contains("<mark class=\"hit-highlight\">5</mark>"));
    }

    #[test]
    fn test_from_match_jyutping_multiple_syllables() {
        let dict = create_test_dict();

        // Search for "lou si" which should match both syllables in 老師
        let results = dict.search("lou si", Box::new(TestStopwatch)).matches;
        assert!(results.len() > 0);

        let result = &results[0];
        let rendered = RenderedResult::from_match(result, &dict);

        // Both syllables should be highlighted
        assert!(rendered.jyutping.contains("<mark class=\"hit-highlight\">lou</mark>"));
        assert!(rendered.jyutping.contains("<mark class=\"hit-highlight\">si</mark>"));
    }

    #[test]
    fn test_from_match_jyutping_substring() {
        let dict = create_test_dict();

        // Search for "saa" which should substring match "saang" in 學生
        let results = dict.search("saa", Box::new(TestStopwatch)).matches;
        assert!(results.len() > 0);

        let result = &results[0];
        assert_eq!(result.entry_id, 1); // 學生 entry

        let rendered = RenderedResult::from_match(result, &dict);

        // Only the matched substring should be highlighted
        assert!(rendered.jyutping.contains("<mark class=\"hit-highlight\">saa</mark>"));
        // The remaining part "ng1" should not be in the mark tags
        assert!(rendered.jyutping.contains("ng1"));
        assert!(!rendered.jyutping.contains("<mark class=\"hit-highlight\">saang1</mark>"));
    }

    #[test]
    fn test_from_match_traditional_highlighting() {
        let dict = create_test_dict();

        // Search for Chinese character
        let results = dict.search("老", Box::new(TestStopwatch)).matches;
        assert!(results.len() > 0);

        let result = &results[0];
        assert!(matches!(result.match_type, MatchType::Traditional));

        let rendered = RenderedResult::from_match(result, &dict);

        // Characters should have highlighting
        assert!(rendered.characters.contains("<mark class=\"hit-highlight\">"));
        assert!(rendered.characters.contains("老</mark>"));

        // Jyutping should not have highlighting
        assert!(!rendered.jyutping.contains("<mark"));

        // English should not have highlighting
        for def in &rendered.english_definitions {
            assert!(!def.contains("<mark"));
        }
    }

    #[test]
    fn test_from_match_traditional_multiple_chars() {
        let dict = create_test_dict();

        // Search for multiple characters
        let results = dict.search("老師", Box::new(TestStopwatch)).matches;
        assert!(results.len() > 0);

        let result = &results[0];
        let rendered = RenderedResult::from_match(result, &dict);

        // Both characters should be highlighted (may be in separate spans)
        assert!(rendered.characters.contains("老"));
        assert!(rendered.characters.contains("師"));
        assert!(rendered.characters.contains("<mark class=\"hit-highlight\">"));
    }

    #[test]
    fn test_from_match_english_highlighting() {
        let dict = create_test_dict();

        // Search for English word
        let results = dict.search("teacher", Box::new(TestStopwatch)).matches;
        assert!(results.len() > 0);

        let result = &results[0];
        assert!(matches!(result.match_type, MatchType::English));

        let rendered = RenderedResult::from_match(result, &dict);

        // English definitions should have highlighting
        let has_highlight = rendered.english_definitions.iter()
            .any(|def| def.contains("<mark class=\"hit-highlight\">"));
        assert!(has_highlight, "English definition should have highlighting");

        // At least one definition should contain highlighted "teach"
        let has_teach = rendered.english_definitions.iter()
            .any(|def| def.contains("teach") && def.contains("<mark"));
        assert!(has_teach, "Should highlight 'teach' in english definitions");

        // Characters should not have highlighting
        assert!(!rendered.characters.contains("<mark"));

        // Jyutping should not have highlighting
        assert!(!rendered.jyutping.contains("<mark"));
    }

    #[test]
    fn test_from_match_english_html_escaping() {
        // Test that HTML in english definitions is properly escaped even with highlighting
        let dict = create_test_dict();

        // Even though our test dict doesn't have HTML in definitions,
        // we can verify the structure is correct
        let results = dict.search("teacher", Box::new(TestStopwatch)).matches;
        if results.len() > 0 {
            let result = &results[0];
            let rendered = RenderedResult::from_match(result, &dict);

            // Verify markup is well-formed: no unescaped < or > outside of mark tags
            for def in &rendered.english_definitions {
                // Count opening and closing mark tags
                let open_marks = def.matches("<mark class=\"hit-highlight\">").count();
                let close_marks = def.matches("</mark>").count();
                assert_eq!(open_marks, close_marks, "Mark tags should be balanced");

                // Any other < or > should be escaped
                let stripped = def.replace("<mark class=\"hit-highlight\">", "")
                    .replace("</mark>", "");
                assert!(!stripped.contains("<") || stripped.contains("&lt;"));
                assert!(!stripped.contains(">") || stripped.contains("&gt;"));
            }
        }
    }

    #[test]
    fn test_from_match_no_highlighting_when_no_spans() {
        let dict = create_test_dict();

        // For match types that don't apply to certain fields,
        // those fields should have no highlighting
        let results = dict.search("lou", Box::new(TestStopwatch)).matches;
        if results.len() > 0 {
            let result = &results[0];
            let rendered = RenderedResult::from_match(result, &dict);

            // This is a Jyutping match, so characters shouldn't be highlighted
            let char_mark_count = rendered.characters.matches("<mark").count();
            assert_eq!(char_mark_count, 0, "Characters should not be highlighted for Jyutping match");
        }
    }

    #[test]
    fn test_from_match_cost_preserved() {
        let dict = create_test_dict();

        let results = dict.search("lou", Box::new(TestStopwatch)).matches;
        assert!(results.len() > 0);

        let result = &results[0];
        let rendered = RenderedResult::from_match(result, &dict);

        // Cost should be preserved from the dictionary entry
        assert_eq!(rendered.cost, dict.entries[result.entry_id].cost);
    }

    #[test]
    fn test_from_match_entry_source_preserved() {
        let dict = create_test_dict();

        let results = dict.search("lou", Box::new(TestStopwatch)).matches;
        assert!(results.len() > 0);

        let result = &results[0];
        let rendered = RenderedResult::from_match(result, &dict);

        // Entry source should be preserved
        assert_eq!(rendered.entry_source, dict.entries[result.entry_id].get_source());
    }
}
