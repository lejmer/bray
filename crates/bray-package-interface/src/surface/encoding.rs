use std::collections::{BTreeMap, BTreeSet};

use bray_symbols::{
    ExternalDeclarationIdentity, ExternalSymbolKey, ExternalSymbolKeyData, InterfaceSymbolId,
    SymbolOrdinal,
};

use super::{InterfaceSymbolReference, PackageInterfaceIdentity, PackageInterfaceSurface};
use crate::InterfaceSectionTag;
use crate::tag::WireTag;
use crate::wire::WireEncoder;

pub(crate) struct EncodedSurfaceSection {
    pub(crate) tag: InterfaceSectionTag,
    pub(crate) record_count: u64,
    pub(crate) payload: Vec<u8>,
}

pub(crate) fn encode_surface(surface: &PackageInterfaceSurface) -> Vec<EncodedSurfaceSection> {
    let strings = StringEncoder::new(surface);

    vec![
        strings.encode_section(),
        encode_metadata(surface, &strings),
        encode_dependencies(surface, &strings),
        encode_symbols(surface, &strings),
        encode_relationships(surface),
        encode_exports(surface, &strings),
    ]
}

struct StringEncoder {
    values: Vec<String>,
    ids: BTreeMap<String, u32>,
}

impl StringEncoder {
    fn new(surface: &PackageInterfaceSurface) -> Self {
        let mut values = BTreeSet::new();

        collect_identity_strings(surface.identity(), &mut values);

        for dependency in surface.dependencies() {
            values.insert(dependency.package().as_str().to_owned());
            values.insert(dependency.product().as_str().to_owned());
        }

        for symbol in surface.symbols().symbols() {
            collect_key_strings(symbol.key(), &mut values);
        }

        for edge in surface.exports() {
            values.insert(edge.name().as_str().to_owned());

            if let InterfaceSymbolReference::Dependency { key, .. } = edge.target() {
                collect_key_strings(key, &mut values);
            }
        }

        let values: Vec<_> = values.into_iter().collect();

        // Encoding owns this short-lived index; cloned strings avoid self-referential storage.
        let ids = values
            .iter()
            .enumerate()
            .map(|(index, value)| (value.clone(), checked_u32(index)))
            .collect();

        Self { values, ids }
    }

    fn id(&self, value: &str) -> u32 {
        match self.ids.get(value) {
            Some(id) => *id,
            None => unreachable!("every encoded string must be collected before indexing"),
        }
    }

    fn encode_section(&self) -> EncodedSurfaceSection {
        let mut encoder = WireEncoder::new();

        for value in &self.values {
            encoder.write_u32(checked_u32(value.len()));
            encoder.write_bytes(value.as_bytes());
        }

        EncodedSurfaceSection {
            tag: InterfaceSectionTag::Strings,
            record_count: checked_u64(self.values.len()),
            payload: encoder.into_bytes(),
        }
    }
}

fn collect_identity_strings(identity: &PackageInterfaceIdentity, values: &mut BTreeSet<String>) {
    values.insert(identity.package().as_str().to_owned());
    values.insert(identity.product().as_str().to_owned());
    values.insert(identity.public_surface().to_owned());
}

fn collect_key_strings(key: &ExternalSymbolKey, values: &mut BTreeSet<String>) {
    let mut current = Some(key);

    while let Some(key) = current {
        match key.data() {
            ExternalSymbolKeyData::Package(package) => {
                values.insert(package.as_str().to_owned());
            }
            ExternalSymbolKeyData::Module { package, path } => {
                values.extend(path.segments().map(str::to_owned));
                current = Some(package);
                continue;
            }
            ExternalSymbolKeyData::Declaration {
                owner, identity, ..
            } => {
                if let ExternalDeclarationIdentity::Name(name) = identity {
                    values.insert(name.as_str().to_owned());
                }
                current = Some(owner);
                continue;
            }
            ExternalSymbolKeyData::Synthesized { owner, .. } => {
                current = Some(owner);
                continue;
            }
        }

        current = None;
    }
}

fn encode_metadata(
    surface: &PackageInterfaceSurface,
    strings: &StringEncoder,
) -> EncodedSurfaceSection {
    let identity = surface.identity();
    let mut encoder = WireEncoder::new();

    encoder.write_u32(strings.id(identity.package().as_str()));
    encoder.write_u32(strings.id(identity.product().as_str()));
    encoder.write_u32(identity.kind().to_wire());
    encoder.write_u32(strings.id(identity.public_surface()));

    encoded_section(InterfaceSectionTag::PackageMetadata, 1, encoder)
}

fn encode_dependencies(
    surface: &PackageInterfaceSurface,
    strings: &StringEncoder,
) -> EncodedSurfaceSection {
    let mut encoder = WireEncoder::new();

    for dependency in surface.dependencies() {
        encoder.write_u32(strings.id(dependency.package().as_str()));
        encoder.write_u32(strings.id(dependency.product().as_str()));
        encoder.write_bytes(dependency.content_hash().as_bytes());
    }

    encoded_section(
        InterfaceSectionTag::Dependencies,
        checked_u64(surface.dependencies().len()),
        encoder,
    )
}

fn encode_symbols(
    surface: &PackageInterfaceSurface,
    strings: &StringEncoder,
) -> EncodedSurfaceSection {
    let mut encoder = WireEncoder::new();

    for symbol in surface.symbols().symbols() {
        encoder.write_u32(symbol.kind().to_wire());
        write_optional_u32(&mut encoder, symbol.container().map(InterfaceSymbolId::raw));
        encode_local_key_component(&mut encoder, symbol.key(), strings);
    }

    encoded_section(
        InterfaceSectionTag::SymbolIdentities,
        checked_u64(surface.symbols().symbols().len()),
        encoder,
    )
}

fn encode_local_key_component(
    encoder: &mut WireEncoder,
    key: &ExternalSymbolKey,
    strings: &StringEncoder,
) {
    match key.data() {
        ExternalSymbolKeyData::Package(package) => {
            encoder.write_u32(1);
            encoder.write_u32(strings.id(package.as_str()));
        }
        ExternalSymbolKeyData::Module { path, .. } => {
            encoder.write_u32(2);
            encoder.write_u32(checked_u32(path.segments().len()));
            for segment in path.segments() {
                encoder.write_u32(strings.id(segment));
            }
        }
        ExternalSymbolKeyData::Declaration { identity, .. } => {
            encoder.write_u32(3);
            encode_declaration_identity(encoder, identity, strings);
        }
        ExternalSymbolKeyData::Synthesized { role, ordinal, .. } => {
            encoder.write_u32(4);
            encoder.write_u32(role.to_wire());
            write_optional_u32(encoder, ordinal.map(SymbolOrdinal::raw));
        }
    }
}

fn encode_declaration_identity(
    encoder: &mut WireEncoder,
    identity: &ExternalDeclarationIdentity,
    strings: &StringEncoder,
) {
    match identity {
        ExternalDeclarationIdentity::Name(name) => {
            encoder.write_u32(1);
            encoder.write_u32(strings.id(name.as_str()));
        }
        ExternalDeclarationIdentity::Ordinal(ordinal) => {
            encoder.write_u32(2);
            encoder.write_u32(ordinal.raw());
        }
    }
}

fn encode_relationships(surface: &PackageInterfaceSurface) -> EncodedSurfaceSection {
    let mut encoder = WireEncoder::new();

    for relationship in surface.relationships() {
        encoder.write_u32(relationship.kind().to_wire());
        encoder.write_u32(relationship.owner().raw());
        encoder.write_u32(relationship.member().raw());
        encoder.write_u32(relationship.ordinal());
    }

    encoded_section(
        InterfaceSectionTag::Relationships,
        checked_u64(surface.relationships().len()),
        encoder,
    )
}

fn encode_exports(
    surface: &PackageInterfaceSurface,
    strings: &StringEncoder,
) -> EncodedSurfaceSection {
    let mut encoder = WireEncoder::new();

    for edge in surface.exports() {
        encoder.write_u32(edge.owner().raw());
        encoder.write_u32(strings.id(edge.name().as_str()));
        encoder.write_u32(edge.kind().to_wire());

        match edge.target() {
            InterfaceSymbolReference::Local(symbol) => {
                encoder.write_u32(1);
                encoder.write_u32(symbol.raw());
            }
            InterfaceSymbolReference::Dependency { dependency, key } => {
                encoder.write_u32(2);
                encoder.write_u32(dependency.raw());
                encode_external_key(&mut encoder, key, strings);
            }
        }
    }

    encoded_section(
        InterfaceSectionTag::ExportedLookup,
        checked_u64(surface.exports().len()),
        encoder,
    )
}

fn encode_external_key(
    encoder: &mut WireEncoder,
    key: &ExternalSymbolKey,
    strings: &StringEncoder,
) {
    let mut components = Vec::new();
    let mut current = Some(key);

    while let Some(key) = current {
        components.push(key);
        current = key.owner();
    }

    components.reverse();

    encoder.write_u32(checked_u32(components.len()));

    for component in components {
        encoder.write_u32(component.kind().to_wire());
        encode_local_key_component(encoder, component, strings);
    }
}

fn encoded_section(
    tag: InterfaceSectionTag,
    record_count: u64,
    encoder: WireEncoder,
) -> EncodedSurfaceSection {
    EncodedSurfaceSection {
        tag,
        record_count,
        payload: encoder.into_bytes(),
    }
}

fn write_optional_u32(encoder: &mut WireEncoder, value: Option<u32>) {
    match value {
        Some(value) => {
            encoder.write_u32(1);
            encoder.write_u32(value);
        }
        None => encoder.write_u32(0),
    }
}

fn checked_u32(value: usize) -> u32 {
    match u32::try_from(value) {
        Ok(value) => value,
        Err(_) => unreachable!("validated interface collection must fit in u32"),
    }
}

fn checked_u64(value: usize) -> u64 {
    match u64::try_from(value) {
        Ok(value) => value,
        Err(_) => unreachable!("interface collection length must fit in u64"),
    }
}

#[cfg(test)]
mod tests {
    use bray_symbols::{
        ExternalSymbolKey, ImportedSymbolIdentityInput, InterfaceSymbolId, ModulePathKey,
        PackageIdentity, SymbolKind, SymbolName,
    };

    use super::{EncodedSurfaceSection, encode_surface};
    use crate::surface::decoding::{IdentitySections, decode_sections};
    use crate::{
        DependencyInterfaceId, ExportedLookupEdge, ExportedLookupKind, InterfaceContentHash,
        InterfaceDependency, InterfaceProductIdentity, InterfaceProductKind, InterfaceSectionTag,
        InterfaceSymbolReference, InterfaceValidationError, InterfaceValidationLimits,
        PackageInterfaceIdentity, PackageInterfaceSurface, SymbolRelationship,
        SymbolRelationshipKind, ValidatedInterfaceSection,
    };

    #[test]
    fn identity_sections_round_trip_every_eager_surface_category() {
        let surface = surface(false);
        let sections = encode_surface(&surface);
        let decoded = decode(&sections, InterfaceValidationLimits::default());

        assert_eq!(decoded, Ok(surface.clone()));
        assert_eq!(
            decoded
                .as_ref()
                .ok()
                .and_then(|surface| surface.exported_lookup(InterfaceSymbolId::new(1), "dep_run"))
                .map(ExportedLookupEdge::target),
            surface
                .exported_lookup(InterfaceSymbolId::new(1), "dep_run")
                .map(ExportedLookupEdge::target)
        );
    }

    #[test]
    fn canonical_encoding_is_stable_for_reordered_collections() {
        let first = encode_surface(&surface(false));
        let second = encode_surface(&surface(true));

        assert_eq!(section_bytes(&first), section_bytes(&second));
    }

    #[test]
    fn malformed_export_references_and_string_limits_are_rejected() {
        let mut sections = encode_surface(&surface(false));

        let Some(exports) = sections
            .iter_mut()
            .find(|section| section.tag == InterfaceSectionTag::ExportedLookup)
        else {
            panic!("export section must be encoded");
        };

        exports.payload[16..20].copy_from_slice(&u32::MAX.to_le_bytes());

        assert_eq!(
            decode(&sections, InterfaceValidationLimits::default()),
            Err(InterfaceValidationError::Malformed)
        );

        let sections = encode_surface(&surface(false));

        assert!(matches!(
            decode(
                &sections,
                InterfaceValidationLimits::default().with_string_length(3)
            ),
            Err(InterfaceValidationError::ResourceLimitExceeded {
                limit: crate::InterfaceLimit::StringLength,
                ..
            })
        ));

        assert!(matches!(
            decode(
                &sections,
                InterfaceValidationLimits::default().with_external_reference_count(2)
            ),
            Err(InterfaceValidationError::ResourceLimitExceeded {
                limit: crate::InterfaceLimit::ExternalReferenceCount,
                ..
            })
        ));
    }

    #[test]
    fn typed_relationship_endpoint_mismatches_are_rejected() {
        let mut sections = encode_surface(&surface(false));

        let Some(relationships) = sections
            .iter_mut()
            .find(|section| section.tag == InterfaceSectionTag::Relationships)
        else {
            panic!("relationship section must be encoded");
        };

        relationships.payload[8..12].copy_from_slice(&0_u32.to_le_bytes());

        assert_eq!(
            decode(&sections, InterfaceValidationLimits::default()),
            Err(InterfaceValidationError::Malformed)
        );
    }

    fn surface(reverse: bool) -> PackageInterfaceSurface {
        let package = package_identity("example.package");
        let product = product_identity("library");

        let Some(identity) = PackageInterfaceIdentity::try_new(
            package.clone(),
            product,
            InterfaceProductKind::Library,
            "surface-v1",
        ) else {
            panic!("test interface identity must be valid");
        };

        let package_key = ExternalSymbolKey::package(package);
        let module_key = module_key(package_key.clone(), "api");

        let Some(struct_key) = ExternalSymbolKey::named(
            module_key.clone(),
            SymbolKind::Struct,
            symbol_name("Widget"),
        ) else {
            panic!("struct key must be valid");
        };

        let Some(field_key) = ExternalSymbolKey::named(
            struct_key.clone(),
            SymbolKind::StructField,
            symbol_name("value"),
        ) else {
            panic!("field key must be valid");
        };

        let symbols = vec![
            ImportedSymbolIdentityInput::new(
                InterfaceSymbolId::new(0),
                package_key,
                SymbolKind::Package,
                None,
            ),
            ImportedSymbolIdentityInput::new(
                InterfaceSymbolId::new(1),
                module_key,
                SymbolKind::Module,
                Some(InterfaceSymbolId::new(0)),
            ),
            ImportedSymbolIdentityInput::new(
                InterfaceSymbolId::new(2),
                struct_key,
                SymbolKind::Struct,
                Some(InterfaceSymbolId::new(1)),
            ),
            ImportedSymbolIdentityInput::new(
                InterfaceSymbolId::new(3),
                field_key,
                SymbolKind::StructField,
                Some(InterfaceSymbolId::new(2)),
            ),
        ];

        let mut dependencies = dependencies();

        let mut relationships = vec![
            SymbolRelationship::new(
                SymbolRelationshipKind::StructField,
                InterfaceSymbolId::new(2),
                InterfaceSymbolId::new(3),
                0,
            ),
            SymbolRelationship::new(
                SymbolRelationshipKind::ModuleMember,
                InterfaceSymbolId::new(1),
                InterfaceSymbolId::new(2),
                0,
            ),
        ];

        let dependency_key = dependency_function_key();
        let dependency = DependencyInterfaceId::new(u32::from(reverse));
        let mut exports = vec![
            ExportedLookupEdge::new(
                InterfaceSymbolId::new(1),
                symbol_name("dep_run"),
                ExportedLookupKind::ReExport,
                InterfaceSymbolReference::Dependency {
                    dependency,
                    key: dependency_key,
                },
            ),
            ExportedLookupEdge::new(
                InterfaceSymbolId::new(1),
                symbol_name("Widget"),
                ExportedLookupKind::Direct,
                InterfaceSymbolReference::Local(InterfaceSymbolId::new(2)),
            ),
        ];

        if reverse {
            dependencies.reverse();
            relationships.reverse();
            exports.reverse();
        }

        match PackageInterfaceSurface::try_new(
            identity,
            dependencies,
            symbols,
            relationships,
            exports,
        ) {
            Ok(surface) => surface,
            Err(error) => panic!("test surface must be valid: {error:?}"),
        }
    }

    fn dependencies() -> Vec<InterfaceDependency> {
        vec![
            InterfaceDependency::new(
                package_identity("dep.package"),
                product_identity("dep-library"),
                InterfaceContentHash::from_bytes([1; 32]),
            ),
            InterfaceDependency::new(
                package_identity("z.package"),
                product_identity("z-library"),
                InterfaceContentHash::from_bytes([2; 32]),
            ),
        ]
    }

    fn dependency_function_key() -> ExternalSymbolKey {
        let package = ExternalSymbolKey::package(package_identity("dep.package"));
        let module = module_key(package, "api");

        match ExternalSymbolKey::named(module, SymbolKind::Function, symbol_name("run")) {
            Some(key) => key,
            None => panic!("dependency function key must be valid"),
        }
    }

    fn module_key(package: ExternalSymbolKey, segment: &str) -> ExternalSymbolKey {
        let Some(path) = ModulePathKey::try_new([segment]) else {
            panic!("module path must be valid");
        };

        match ExternalSymbolKey::module(package, path) {
            Some(key) => key,
            None => panic!("module key must be valid"),
        }
    }

    fn package_identity(value: &str) -> PackageIdentity {
        match PackageIdentity::try_new(value) {
            Some(identity) => identity,
            None => panic!("package identity must be valid"),
        }
    }

    fn product_identity(value: &str) -> InterfaceProductIdentity {
        match InterfaceProductIdentity::try_new(value) {
            Some(identity) => identity,
            None => panic!("product identity must be valid"),
        }
    }

    fn symbol_name(value: &str) -> SymbolName {
        match SymbolName::try_new(value) {
            Some(name) => name,
            None => panic!("symbol name must be valid"),
        }
    }

    fn decode(
        sections: &[EncodedSurfaceSection],
        limits: InterfaceValidationLimits,
    ) -> Result<PackageInterfaceSurface, InterfaceValidationError> {
        let view = |tag| {
            let Some(section) = sections.iter().find(|section| section.tag == tag) else {
                panic!("required test section must exist: {tag:?}");
            };

            ValidatedInterfaceSection::for_test(section.tag, section.record_count, &section.payload)
        };

        decode_sections(
            IdentitySections {
                strings: view(InterfaceSectionTag::Strings),
                metadata: view(InterfaceSectionTag::PackageMetadata),
                dependencies: view(InterfaceSectionTag::Dependencies),
                symbols: view(InterfaceSectionTag::SymbolIdentities),
                relationships: view(InterfaceSectionTag::Relationships),
                exports: view(InterfaceSectionTag::ExportedLookup),
            },
            limits,
        )
    }

    fn section_bytes(sections: &[EncodedSurfaceSection]) -> Vec<(InterfaceSectionTag, Vec<u8>)> {
        sections
            .iter()
            .map(|section| (section.tag, section.payload.clone()))
            .collect()
    }
}
