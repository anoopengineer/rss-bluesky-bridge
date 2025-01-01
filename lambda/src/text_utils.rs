use unicode_segmentation::UnicodeSegmentation;

#[must_use]
pub fn truncate_to_word(summary: &str, max_graphemes: usize) -> String {
    if max_graphemes == 0 || summary.trim().is_empty() {
        return summary.trim().to_string();
    }
    let summary = summary.trim();
    let num_graphemes = summary.graphemes(true).count();
    // Trim the summary to MAX_BSKY_GRAPHEMES graphemes
    if num_graphemes > max_graphemes {
        let mut graphemes = summary.graphemes(true).collect::<Vec<&str>>();
        graphemes.truncate(max_graphemes);

        // Find the last space to avoid cutting words
        if let Some(last_space_index) = graphemes.iter().rposition(|&g| g == " ") {
            // If we found a space, use it as the cut-off point
            if last_space_index > 0 {
                graphemes.truncate(last_space_index);
            }
        }
        // Remove trailing spaces
        while graphemes.last().map_or(false, |&g| g == " ") {
            graphemes.pop();
        }

        let trimmed_length = graphemes.len();

        if trimmed_length < max_graphemes {
            // If we have room, add the ellipsis
            graphemes.push("…");
        } else if trimmed_length == max_graphemes {
            // If we're at the limit, replace the last grapheme with an ellipsis
            *graphemes.last_mut().unwrap() = "…";
        }

        graphemes.join("")
    } else {
        summary.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_to_word() {
        // Basic cases
        assert_eq!(truncate_to_word("Hello", 5), "Hello");
        assert_eq!(truncate_to_word("Hello, world!", 5), "Hell…");
        assert_eq!(truncate_to_word("Short", 10), "Short");

        // Edge cases with max_graphemes
        assert_eq!(truncate_to_word("Any text", 0), "Any text");
        assert_eq!(truncate_to_word("A", 1), "A");
        assert_eq!(truncate_to_word("Ab", 1), "…");

        // Exact length matches
        assert_eq!(truncate_to_word("Exactly", 7), "Exactly");
        assert_eq!(truncate_to_word("Exactly!", 7), "Exactl…");

        // Spaces and punctuation
        assert_eq!(truncate_to_word("Hello, world!", 7), "Hello,…");
        assert_eq!(truncate_to_word("Hello world", 11), "Hello world");
        assert_eq!(truncate_to_word("Hello world", 10), "Hello…");

        // Multiple spaces
        assert_eq!(truncate_to_word("Hello   world", 8), "Hello…");

        // All spaces
        assert_eq!(truncate_to_word("    ", 2), "");

        // Unicode characters
        assert_eq!(truncate_to_word("こんにちは世界", 5), "こんにち…");
        assert_eq!(truncate_to_word("🌍🌎🌏", 2), "🌍…");

        // Mixed ASCII and Unicode
        assert_eq!(truncate_to_word("Hello 世界", 7), "Hello…");

        // Long word at the start
        assert_eq!(
            truncate_to_word("Supercalifragilisticexpialidocious is long", 10),
            "Supercali…"
        );

        // No spaces
        assert_eq!(truncate_to_word("NoSpacesHere", 5), "NoSp…");

        // Empty string
        assert_eq!(truncate_to_word("", 5), "");

        // Only ellipsis fits
        assert_eq!(truncate_to_word("Too long", 1), "…");

        // Trailing spaces
        assert_eq!(truncate_to_word("Trailing spaces   ", 10), "Trailing…");

        // Leading spaces
        assert_eq!(truncate_to_word("   Leading spaces", 10), "Leading…");

        // Exactly one character over
        assert_eq!(truncate_to_word("Exactly_one_over", 15), "Exactly_one_ov…");

        // Max length is the length of the string
        assert_eq!(truncate_to_word("Exact", 5), "Exact");

        // Max length is one less than the string length
        assert_eq!(truncate_to_word("Almost", 5), "Almo…");

        // String with newlines
        assert_eq!(truncate_to_word("Line_1\nLine_2", 7), "Line_1…");

        // String with tabs
        assert_eq!(truncate_to_word("Tab\tSeparated", 5), "Tab\t…");
    }
}
