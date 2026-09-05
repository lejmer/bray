/// Rejects duplicate textual identities and native symbols during constant evaluation.
pub(crate) const fn validate_identities(groups: &[&[(&str, Option<&str>)]]) {
    let mut group = 0;

    while group < groups.len() {
        let mut index = 0;

        while index < groups[group].len() {
            let (name, symbol) = groups[group][index];

            assert!(!name.is_empty(), "empty role identity in runtime catalog");

            if let Some(symbol) = symbol {
                assert!(!symbol.is_empty(), "empty native symbol in runtime catalog");
            }

            let mut previous_group = 0;

            while previous_group <= group {
                let limit = if previous_group == group {
                    index
                } else {
                    groups[previous_group].len()
                };

                let mut previous = 0;

                while previous < limit {
                    let (other_name, other_symbol) = groups[previous_group][previous];

                    assert!(
                        !equal(name, other_name),
                        "duplicate role identity in runtime catalog"
                    );

                    if let (Some(symbol), Some(other)) = (symbol, other_symbol) {
                        assert!(
                            !equal(symbol, other),
                            "duplicate native symbol in runtime catalog"
                        );
                    }

                    previous += 1;
                }

                previous_group += 1;
            }

            index += 1;
        }

        group += 1;
    }
}

pub(crate) const fn equal(left: &str, right: &str) -> bool {
    let left = left.as_bytes();
    let right = right.as_bytes();

    if left.len() != right.len() {
        return false;
    }

    let mut index = 0;

    while index < left.len() {
        if left[index] != right[index] {
            return false;
        }

        index += 1;
    }

    true
}

#[cfg(test)]
mod tests {
    use super::validate_identities;

    #[test]
    #[should_panic(expected = "duplicate role identity")]
    fn duplicate_role_names_are_rejected() {
        validate_identities(&[&[("root", Some("first")), ("root", Some("second"))]]);
    }

    #[test]
    #[should_panic(expected = "duplicate native symbol")]
    fn duplicate_native_symbols_are_rejected() {
        validate_identities(&[&[("root", Some("shared"))], &[("task", Some("shared"))]]);
    }

    #[test]
    fn compiler_owned_roles_need_no_native_symbol() {
        validate_identities(&[&[("root", Some("root")), ("frame", None), ("generator", None)]]);
    }
}
