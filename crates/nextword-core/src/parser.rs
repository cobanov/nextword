//! Raw llama-server completion -> deduped, trimmed single-word suggestions.

/// Parse a batch of raw completions plus the context they were generated from
/// into final user-visible suggestions.
pub fn parse_suggestions(raw: &[String], context: &str) -> Vec<String> {
    let last_word_lower = context
        .split_whitespace()
        .next_back()
        .map(|w| w.to_lowercase());

    let capitalize_next = should_capitalize(context);

    let mut seen: Vec<String> = Vec::with_capacity(raw.len());
    for r in raw {
        let cleaned = clean_token(r);
        if cleaned.is_empty() {
            continue;
        }
        let final_form = if capitalize_next {
            capitalize_first(&cleaned)
        } else {
            cleaned.to_lowercase()
        };

        let lower = final_form.to_lowercase();
        if Some(&lower) == last_word_lower.as_ref() {
            continue;
        }
        if seen.iter().any(|s| s.to_lowercase() == lower) {
            continue;
        }
        seen.push(final_form);
        if seen.len() >= 3 {
            break;
        }
    }
    seen
}

fn clean_token(raw: &str) -> String {
    let trimmed = raw.trim_start();
    let mut out = String::with_capacity(trimmed.len());
    for c in trimmed.chars() {
        if c.is_alphabetic() || c == '\'' {
            out.push(c);
        } else {
            break;
        }
    }
    out
}

fn should_capitalize(context: &str) -> bool {
    let trimmed = context.trim_end_matches(' ');
    let last = trimmed.chars().next_back();
    matches!(last, Some('.') | Some('!') | Some('?')) || trimmed.is_empty()
}

fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str().to_lowercase().as_str(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_leading_whitespace() {
        let out = parse_suggestions(&["  store".into()], "I went to the ");
        assert_eq!(out, vec!["store"]);
    }

    #[test]
    fn cuts_at_punctuation() {
        let out = parse_suggestions(&["dog.".into()], "the ");
        assert_eq!(out, vec!["dog"]);
    }

    #[test]
    fn dedupes_case_insensitive() {
        let out = parse_suggestions(
            &["store".into(), "Store".into(), "shop".into()],
            "I went to the ",
        );
        assert_eq!(out, vec!["store", "shop"]);
    }

    #[test]
    fn drops_repeat_of_last_word() {
        let out = parse_suggestions(
            &["dog".into(), "cat".into()],
            "I have a dog ",
        );
        assert_eq!(out, vec!["cat"]);
    }

    #[test]
    fn capitalizes_after_sentence_end() {
        let out = parse_suggestions(&["hello".into()], "It is done. ");
        assert_eq!(out, vec!["Hello"]);
    }

    #[test]
    fn lowercases_mid_sentence() {
        let out = parse_suggestions(&["Hello".into()], "I said ");
        assert_eq!(out, vec!["hello"]);
    }
}
