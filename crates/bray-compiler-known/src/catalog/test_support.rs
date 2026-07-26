use bray_source::{TextRange, TextSize};

use super::{
    CatalogDeclarationSurface, CatalogSourceAnchor, CompilerKnownDeclarationKey,
    generator_input_inventory,
};

pub(crate) fn declaration_surface() -> CatalogDeclarationSurface {
    let source = generator_input_inventory().sources()[0].id();

    let anchor = CatalogSourceAnchor {
        source,
        range: TextRange::new(TextSize::new(1), TextSize::new(5)),
    };

    CatalogDeclarationSurface(anchor)
}

pub(crate) fn declaration_key(value: &str) -> CompilerKnownDeclarationKey {
    match CompilerKnownDeclarationKey::try_new(value) {
        Some(key) => key,
        None => panic!("test declaration key is valid"),
    }
}
