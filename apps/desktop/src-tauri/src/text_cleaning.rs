//! Pure selected-text normalization shared by the desktop runtime.
//!
//! The ASCII hyphen heuristic intentionally mirrors `packages/core/src/text.ts`:
//! a wrapped English word is repaired only when the left fragment has at least
//! two ASCII letters, and known compound-word shapes retain their hyphen.

const KEEP_HYPHEN_RIGHT_PREFIXES: [&str; 15] = [
    "of", "the", "and", "or", "to", "in", "on", "based", "driven", "free", "aware", "level",
    "scale", "specific", "related",
];

/// Cleans text captured from an accessibility selection.
///
/// When cleaning is disabled, this deliberately performs only the same
/// leading/trailing trim as the TypeScript implementation. In particular, it
/// does not rewrite internal line endings or whitespace.
pub(crate) fn clean_selected_text(input: &str, enabled: bool) -> String {
    if !enabled {
        return input.trim().to_owned();
    }

    let normalized = normalize_line_endings(input);
    let repaired = repair_wrapped_words(&normalized);
    collapse_layout_whitespace(&repaired).trim().to_owned()
}

fn normalize_line_endings(input: &str) -> String {
    let mut normalized = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\r' {
            if chars.peek() == Some(&'\n') {
                chars.next();
            }
            normalized.push('\n');
        } else {
            normalized.push(ch);
        }
    }

    normalized
}

fn repair_wrapped_words(input: &str) -> String {
    let chars = input.chars().collect::<Vec<_>>();
    let mut repaired = String::with_capacity(input.len());
    let mut index = 0;

    while index < chars.len() {
        let connector = chars[index];

        if is_wrapping_connector(connector) {
            if let Some((right_start, right_end)) = wrapped_word_right_range(&chars, index) {
                let right = chars[right_start..right_end].iter().collect::<String>();

                if connector != '\u{00ad}' && (connector != '-' || should_keep_ascii_hyphen(&right))
                {
                    repaired.push(connector);
                }

                index = right_start;
                continue;
            }

            // U+00AD is a discretionary rendering hint, not textual content.
            // It is removed even when extraction did not retain a line break.
            if connector == '\u{00ad}' {
                index += 1;
                continue;
            }
        }

        repaired.push(connector);
        index += 1;
    }

    repaired
}

fn wrapped_word_right_range(chars: &[char], connector_index: usize) -> Option<(usize, usize)> {
    let left_length = chars[..connector_index]
        .iter()
        .rev()
        .take_while(|ch| ch.is_ascii_alphabetic())
        .count();

    if left_length < 2 {
        return None;
    }

    let mut right_start = connector_index + 1;
    let mut contains_line_break = false;

    while right_start < chars.len() && chars[right_start].is_whitespace() {
        contains_line_break |= chars[right_start] == '\n';
        right_start += 1;
    }

    if !contains_line_break
        || right_start >= chars.len()
        || !chars[right_start].is_ascii_alphabetic()
    {
        return None;
    }

    let mut right_end = right_start + 1;
    while right_end < chars.len()
        && (chars[right_end].is_ascii_alphabetic() || chars[right_end] == '-')
    {
        right_end += 1;
    }

    Some((right_start, right_end))
}

fn is_wrapping_connector(ch: char) -> bool {
    matches!(
        ch,
        '-'             // HYPHEN-MINUS
            | '\u{00ad}' // SOFT HYPHEN
            | '\u{058a}' // ARMENIAN HYPHEN
            | '\u{2010}' // HYPHEN
            | '\u{2011}' // NON-BREAKING HYPHEN
            | '\u{2e17}' // DOUBLE OBLIQUE HYPHEN
            | '\u{30a0}' // KATAKANA-HIRAGANA DOUBLE HYPHEN
            | '\u{fe63}' // SMALL HYPHEN-MINUS
            | '\u{ff0d}' // FULLWIDTH HYPHEN-MINUS
    )
}

fn should_keep_ascii_hyphen(right: &str) -> bool {
    if right.chars().count() <= 1 {
        return true;
    }

    if let Some(hyphen_index) = right.find('-') {
        let before = &right[..hyphen_index];
        let after = &right[hyphen_index + 1..];
        if !before.is_empty()
            && before.chars().all(|ch| ch.is_ascii_alphabetic())
            && !after.is_empty()
            && after
                .chars()
                .all(|ch| ch.is_ascii_alphabetic() || ch == '-')
        {
            return true;
        }
    }

    KEEP_HYPHEN_RIGHT_PREFIXES.iter().any(|prefix| {
        let Some(candidate) = right.get(..prefix.len()) else {
            return false;
        };

        candidate.eq_ignore_ascii_case(prefix)
            && right[prefix.len()..]
                .chars()
                .next()
                .is_none_or(|ch| ch == '-')
    })
}

fn collapse_layout_whitespace(input: &str) -> String {
    let mut collapsed = String::with_capacity(input.len());
    let mut in_collapsible_run = false;

    for ch in input.chars() {
        if matches!(ch, ' ' | '\t' | '\n' | '\u{000b}' | '\u{000c}') {
            if !in_collapsible_run {
                collapsed.push(' ');
                in_collapsible_run = true;
            }
        } else {
            collapsed.push(ch);
            in_collapsible_run = false;
        }
    }

    collapsed
}

#[cfg(test)]
mod tests {
    use super::clean_selected_text;

    #[test]
    fn repairs_pdf_hyphenation_from_typescript_golden_case() {
        assert_eq!(
            clean_selected_text("The pro-\nposed method works.", true),
            "The proposed method works."
        );
    }

    #[test]
    fn keeps_real_compound_from_typescript_golden_case() {
        assert_eq!(
            clean_selected_text("state-\nof-the-art performance", true),
            "state-of-the-art performance"
        );
    }

    #[test]
    fn collapses_newlines_and_spaces_from_typescript_golden_case() {
        assert_eq!(
            clean_selected_text("line one\n  line two    line three", true),
            "line one line two line three"
        );
    }

    #[test]
    fn disabled_cleaning_matches_typescript_golden_case() {
        assert_eq!(
            clean_selected_text("line one\nline two", false),
            "line one\nline two"
        );
    }

    #[test]
    fn disabled_cleaning_only_trims_the_edges() {
        assert_eq!(
            clean_selected_text(" \tfirst\r\n  second\u{000b}  ", false),
            "first\r\n  second"
        );
    }

    #[test]
    fn normalizes_crlf_and_bare_cr_before_repairing() {
        assert_eq!(
            clean_selected_text("pro-\r\nposed and con-\rcluded", true),
            "proposed and concluded"
        );
    }

    #[test]
    fn removes_soft_hyphen_at_a_line_break_or_inline() {
        assert_eq!(
            clean_selected_text("inter\u{00ad}\nnational co\u{00ad}operate", true),
            "international cooperate"
        );
    }

    #[test]
    fn preserves_visible_unicode_hyphens_across_a_line_break() {
        assert_eq!(
            clean_selected_text("well\u{2010}\nbeing and non\u{2011}\nbreaking", true),
            "well\u{2010}being and non\u{2011}breaking"
        );
    }

    #[test]
    fn keeps_known_ascii_compound_boundaries() {
        assert_eq!(
            clean_selected_text("evidence-\nbased and risk-\naware", true),
            "evidence-based and risk-aware"
        );
        assert_eq!(clean_selected_text("section-\na", true), "section-a");
    }

    #[test]
    fn does_not_remove_hyphens_without_the_typescript_word_shape() {
        assert_eq!(
            clean_selected_text("x-\naxis 中文-\n换行", true),
            "x- axis 中文- 换行"
        );
    }

    #[test]
    fn handles_chinese_paragraphs_and_punctuation() {
        assert_eq!(
            clean_selected_text("第一段，\n  包含标点。\n\n第二段！", true),
            "第一段， 包含标点。 第二段！"
        );
    }

    #[test]
    fn collapses_supported_horizontal_whitespace() {
        assert_eq!(
            clean_selected_text("alpha\t \u{000b}\u{000c} beta", true),
            "alpha beta"
        );
    }

    #[test]
    fn preserves_non_layout_unicode_whitespace_inside_text() {
        assert_eq!(
            clean_selected_text("alpha\u{00a0}\u{00a0}beta", true),
            "alpha\u{00a0}\u{00a0}beta"
        );
    }

    #[test]
    fn handles_empty_and_whitespace_only_input() {
        assert_eq!(clean_selected_text("", true), "");
        assert_eq!(clean_selected_text(" \t\r\n\u{000c} ", true), "");
        assert_eq!(clean_selected_text(" \t\r\n ", false), "");
    }
}
