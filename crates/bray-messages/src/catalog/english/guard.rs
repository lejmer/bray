pub(crate) fn forbidden_internal_term(message: &str) -> Option<&'static str> {
    let normalized = message.to_ascii_lowercase();

    if normalized == "query" {
        // Compiler-profile descriptor vocabulary is documented by the opt-in profiling report
        // and is the only ordinary-catalog exception.
        return None;
    }

    [
        "fact",
        "facts",
        "witness",
        "query",
        "queries",
        "skeleton",
        "interned",
        "compiler coordination",
        "belongs to store",
        "semantic store",
        "value-store",
    ]
    .into_iter()
    .find(|term| contains_word_or_phrase(&normalized, term))
}

pub(super) fn forbidden_profile_term(message: &str) -> Option<&'static str> {
    forbidden_internal_term(message).filter(|term| !matches!(*term, "query" | "queries"))
}

pub(crate) fn forbidden_ordinary_diagnostic_term(message: &str) -> Option<&'static str> {
    forbidden_internal_term(message).or_else(|| {
        let normalized = message.to_ascii_lowercase();

        [
            "semantic unit",
            "semantic context",
            "mir",
            "codegen",
            "code generation unit",
            "code generation instance",
            "emission plan",
            "artifact contribution",
            "realization mapping",
            "backend",
            "partition",
            "demanded type",
        ]
        .into_iter()
        .find(|term| contains_word_or_phrase(&normalized, term))
    })
}

fn contains_word_or_phrase(message: &str, term: &str) -> bool {
    message.match_indices(term).any(|(start, matched)| {
        let end = start + matched.len();

        let before_is_word = message[..start]
            .chars()
            .next_back()
            .is_some_and(is_word_character);

        let after_is_word = message[end..].chars().next().is_some_and(is_word_character);

        !before_is_word && !after_is_word
    })
}

const fn is_word_character(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '_'
}
