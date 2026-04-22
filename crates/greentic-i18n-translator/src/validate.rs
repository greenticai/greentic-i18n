use crate::json_map::JsonMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    MissingKey,
    EmptyTranslationNotAllowed,
    PlaceholderCountMismatch {
        expected: usize,
        found: usize,
    },
    NewlineCountMismatch {
        expected: usize,
        found: usize,
    },
    BacktickSpansMismatch {
        expected: Vec<String>,
        found: Vec<String>,
    },
}

impl ValidationError {
    pub fn message(&self) -> String {
        match self {
            ValidationError::MissingKey => "missing key in translation file".to_string(),
            ValidationError::EmptyTranslationNotAllowed => {
                "translation is empty while English source is non-empty".to_string()
            }
            ValidationError::PlaceholderCountMismatch { expected, found } => {
                format!("placeholder count mismatch for `{{}}`: expected {expected}, found {found}")
            }
            ValidationError::NewlineCountMismatch { expected, found } => {
                format!("newline count mismatch: expected {expected}, found {found}")
            }
            ValidationError::BacktickSpansMismatch { expected, found } => {
                format!(
                    "backtick spans mismatch: expected {:?}, found {:?}",
                    expected, found
                )
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationIssue {
    pub key: String,
    pub error: ValidationError,
}

pub fn count_placeholders_positional(s: &str) -> usize {
    s.matches("{}").count()
}

pub fn extract_backtick_spans(s: &str) -> Vec<String> {
    let mut spans = Vec::new();
    let mut start: Option<usize> = None;
    for (idx, ch) in s.char_indices() {
        if ch == '`' {
            match start {
                None => start = Some(idx + ch.len_utf8()),
                Some(open_idx) => {
                    spans.push(s[open_idx..idx].to_string());
                    start = None;
                }
            }
        }
    }
    spans
}

pub fn count_newlines_normalized(s: &str) -> usize {
    s.chars().filter(|&ch| ch == '\n').count()
}

pub fn validate_translation(en: &str, tr: &str) -> Result<(), ValidationError> {
    if !en.is_empty() && tr.is_empty() {
        return Err(ValidationError::EmptyTranslationNotAllowed);
    }

    let expected_placeholders = count_placeholders_positional(en);
    let found_placeholders = count_placeholders_positional(tr);
    if expected_placeholders != found_placeholders {
        return Err(ValidationError::PlaceholderCountMismatch {
            expected: expected_placeholders,
            found: found_placeholders,
        });
    }

    let expected_newlines = count_newlines_normalized(en);
    let found_newlines = count_newlines_normalized(tr);
    if expected_newlines != found_newlines {
        return Err(ValidationError::NewlineCountMismatch {
            expected: expected_newlines,
            found: found_newlines,
        });
    }

    let expected_backticks = extract_backtick_spans(en);
    let found_backticks = extract_backtick_spans(tr);
    if expected_backticks != found_backticks {
        return Err(ValidationError::BacktickSpansMismatch {
            expected: expected_backticks,
            found: found_backticks,
        });
    }

    Ok(())
}

pub fn validate_lang_map(en_map: &JsonMap, tr_map: &JsonMap) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();
    for (key, en_value) in en_map {
        let Some(tr_value) = tr_map.get(key) else {
            issues.push(ValidationIssue {
                key: key.clone(),
                error: ValidationError::MissingKey,
            });
            continue;
        };

        if let Err(error) = validate_translation(en_value, tr_value) {
            issues.push(ValidationIssue {
                key: key.clone(),
                error,
            });
        }
    }
    issues
}

#[cfg(test)]
mod tests {
    use super::{
        ValidationError, ValidationIssue, count_newlines_normalized, count_placeholders_positional,
        extract_backtick_spans, validate_lang_map, validate_translation,
    };
    use crate::json_map::JsonMap;

    #[test]
    fn placeholder_mismatch_fails() {
        let err = validate_translation("raw:\n{}", "raw:\n")
            .expect_err("placeholder mismatch should fail validation");
        assert_eq!(
            err,
            ValidationError::PlaceholderCountMismatch {
                expected: 1,
                found: 0
            }
        );
    }

    #[test]
    fn backtick_span_exact_preservation_is_required() {
        let err = validate_translation("Use `--flag` now", "Utilisez `--drapeau` maintenant")
            .expect_err("changed backtick content should fail");
        assert_eq!(
            err,
            ValidationError::BacktickSpansMismatch {
                expected: vec!["--flag".to_string()],
                found: vec!["--drapeau".to_string()],
            }
        );
    }

    #[test]
    fn newline_count_mismatch_fails() {
        let err = validate_translation("a\nb\n{}", "a b {}")
            .expect_err("newline mismatch should fail validation");
        assert_eq!(
            err,
            ValidationError::NewlineCountMismatch {
                expected: 2,
                found: 0
            }
        );
    }

    #[test]
    fn helper_extractors_work() {
        assert_eq!(count_placeholders_positional("{} a {}"), 2);
        assert_eq!(count_newlines_normalized("a\nb\n"), 2);
        assert_eq!(
            extract_backtick_spans("do `cmd --x` and `cmd --y`"),
            vec!["cmd --x".to_string(), "cmd --y".to_string()]
        );
    }

    #[test]
    fn empty_translation_is_rejected_for_non_empty_english() {
        let err = validate_translation("hello", "").expect_err("empty translation should fail");
        assert_eq!(err, ValidationError::EmptyTranslationNotAllowed);
        assert_eq!(
            err.message(),
            "translation is empty while English source is non-empty"
        );
    }

    #[test]
    fn validate_lang_map_reports_missing_keys() {
        let mut en_map = JsonMap::new();
        en_map.insert("hello".to_string(), "Hello".to_string());
        let tr_map = JsonMap::new();

        assert_eq!(
            validate_lang_map(&en_map, &tr_map),
            vec![ValidationIssue {
                key: "hello".to_string(),
                error: ValidationError::MissingKey,
            }]
        );
    }
}
