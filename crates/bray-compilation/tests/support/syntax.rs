use std::collections::BTreeMap;

pub(crate) struct EnumInventory {
    variants: BTreeMap<String, Vec<String>>,
}

impl EnumInventory {
    pub(crate) fn new(rust: &BTreeMap<String, String>) -> Self {
        let mut variants: BTreeMap<String, Vec<String>> = BTreeMap::new();

        for (path, contents) in rust {
            let file = syn::parse_file(contents)
                .unwrap_or_else(|error| panic!("could not parse {path}: {error}"));

            for item in file.items {
                let syn::Item::Enum(item) = item else {
                    continue;
                };

                variants
                    .entry(item.ident.to_string())
                    .or_default()
                    .extend(
                        item.variants
                            .into_iter()
                            .map(|variant| variant.ident.to_string()),
                    );
            }
        }

        for variants in variants.values_mut() {
            variants.sort();
        }

        Self { variants }
    }

    pub(crate) fn variants(&self, enum_name: &str) -> &[String] {
        let Some(variants) = self.variants.get(enum_name) else {
            panic!("could not find enum {enum_name} in the Rust workspace");
        };

        variants
    }
}
