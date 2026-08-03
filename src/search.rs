use std::ops::Range;

use regex::RegexBuilder;

#[derive(Debug, Clone, Copy, Default)]
pub struct Options {
    pub case_sensitive: bool,
    pub whole_word: bool,
}

pub fn find_matches(text: &str, query: &str, options: Options) -> Vec<Range<usize>> {
    if query.is_empty() {
        return Vec::new();
    }

    let Ok(pattern) = RegexBuilder::new(&regex::escape(query))
        .case_insensitive(!options.case_sensitive)
        .build()
    else {
        return Vec::new();
    };

    pattern
        .find_iter(text)
        .filter(|found| !options.whole_word || is_whole_word(text, found.range(), query))
        .map(|found| found.range())
        .collect()
}

fn is_whole_word(text: &str, range: Range<usize>, query: &str) -> bool {
    let starts_with_word = query.chars().next().is_some_and(is_word_character);
    let ends_with_word = query.chars().next_back().is_some_and(is_word_character);
    let previous_is_word = text[..range.start]
        .chars()
        .next_back()
        .is_some_and(is_word_character);
    let next_is_word = text[range.end..]
        .chars()
        .next()
        .is_some_and(is_word_character);

    !(starts_with_word && previous_is_word || ends_with_word && next_is_word)
}

fn is_word_character(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_query_has_no_matches() {
        assert!(find_matches("texto", "", Options::default()).is_empty());
    }

    #[test]
    fn case_insensitive_search_preserves_original_byte_ranges() {
        let text = "Árvore e árvore";
        let matches = find_matches(text, "árvore", Options::default());

        assert_eq!(matches.len(), 2);
        assert_eq!(&text[matches[0].clone()], "Árvore");
        assert_eq!(&text[matches[1].clone()], "árvore");
    }

    #[test]
    fn whole_word_rejects_embedded_and_underscore_matches() {
        let text = "cat scatter cat_ cat.";
        let matches = find_matches(
            text,
            "cat",
            Options {
                whole_word: true,
                ..Options::default()
            },
        );

        assert_eq!(matches, vec![0..3, 17..20]);
    }

    #[test]
    fn case_sensitive_search_distinguishes_case() {
        let matches = find_matches(
            "Typst typst",
            "Typst",
            Options {
                case_sensitive: true,
                ..Options::default()
            },
        );

        assert_eq!(matches, vec![0..5]);
    }
}
