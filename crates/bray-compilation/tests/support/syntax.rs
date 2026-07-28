use std::collections::BTreeMap;

pub(crate) fn enum_variants(
    rust: &BTreeMap<String, String>,
    enum_name: &str,
) -> Vec<String> {
    let mut variants = Vec::new();

    for (path, contents) in rust {
        let file = syn::parse_file(contents)
            .unwrap_or_else(|error| panic!("could not parse {path}: {error}"));

        for item in file.items {
            let syn::Item::Enum(item) = item else {
                continue;
            };

            if item.ident == enum_name {
                variants.extend(
                    item.variants
                        .into_iter()
                        .map(|variant| variant.ident.to_string()),
                );
            }
        }
    }

    variants.sort();

    assert!(
        !variants.is_empty(),
        "could not find enum {enum_name} in the Rust workspace"
    );

    variants
}
