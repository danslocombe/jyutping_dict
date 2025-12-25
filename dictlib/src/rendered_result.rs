use serde::Serialize;
use crate::compiled_dictionary::{CompiledDictionary, CompiledDictionaryEntry, Match, MatchType};
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
    pub fn from_match(match_result: &Match, dict: &CompiledDictionary) -> Self {
        let entry = &dict.entries[match_result.entry_id];

        let mut characters =
        {
            let mut characters = String::new();
            for c in &entry.characters {
                characters.push(dict.character_store.characters[*c as usize]);
            }
            characters
        };

        if let MatchType::Traditional = match_result.match_type {
            characters = Self::apply_highlights(&characters, &match_result.matched_spans);
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

        if let MatchType::Jyutping = match_result.match_type {
            jyutping = Self::apply_highlights(&jyutping, &match_result.matched_spans);
        }

        let english_definitions = if let MatchType::English = match_result.match_type {
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
            for (span_start_abs, span_end_abs) in matched_spans {
                if (*span_start_abs < start)
                {
                    continue;
                }

                if (*span_end_abs > end)
                {
                    continue;
                }

                filtered_modified_matches.push((*span_start_abs - start, *span_end_abs - start));
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

        for (start, end) in matched_spans {
            let start = *start;
            let end = *end;

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
}
