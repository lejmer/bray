use std::hash::Hasher;

use bray_base::StableDigestHasher;

use super::{CompilationProfileSubjectKind, ProfileQueryKind};
use crate::fact::CompilationFactKey;

#[derive(Clone, Copy, Debug)]
pub(super) struct ProfileSubjectRecord {
    pub(super) kind: CompilationProfileSubjectKind,
    pub(super) fingerprint: u64,
}

pub(super) fn profile_subject(
    query: ProfileQueryKind,
    key: &CompilationFactKey,
) -> ProfileSubjectRecord {
    let kind = match key {
        CompilationFactKey::SourceUnitSyntax(_)
        | CompilationFactKey::SourceReferenceIndex(_)
        | CompilationFactKey::DeclarationChunk(_) => CompilationProfileSubjectKind::SourceUnit,
        CompilationFactKey::CodegenArtifact(_) => CompilationProfileSubjectKind::CodegenUnit,
        CompilationFactKey::NativeProduct(_)
        | CompilationFactKey::PackageInterfaceExportBundle
        | CompilationFactKey::ProductSourceGraph
        | CompilationFactKey::ProductSemantics
        | CompilationFactKey::TestDiscovery(_) => CompilationProfileSubjectKind::Product,
        key if key.bound_unit_key().is_some() => CompilationProfileSubjectKind::SemanticUnit,
        _ => CompilationProfileSubjectKind::Compilation,
    };

    let mut encoder = StableSubjectEncoder::new();

    encoder.write_u16(query.id());
    encoder.write_u8(subject_kind_code(kind));

    match key {
        CompilationFactKey::SourceUnitSyntax(source)
        | CompilationFactKey::SourceReferenceIndex(source)
        | CompilationFactKey::DeclarationChunk(source) => encoder.write_u32(source.raw()),
        CompilationFactKey::CodegenArtifact(key) => {
            encoder.write_bytes(&key.unit().content_identity());
        }
        CompilationFactKey::NativeProduct(key) => encode_product(&mut encoder, key.product()),
        CompilationFactKey::TestDiscovery(product) => encode_product(&mut encoder, product),
        key => {
            if let Some(unit) = key.bound_unit_key() {
                let source = unit.source();
                let syntax = source.syntax();
                let range = syntax.full_range();

                encoder.write_str(unit.kind().as_str());
                encoder.write_u32(syntax.source_id().raw());
                encoder.write_str(syntax.syntax_kind().as_str());
                encoder.write_u32(range.start().bytes());
                encoder.write_u32(range.end().bytes());
                encoder.write_u8(u8::from(syntax.is_recovered()));
                encoder.write_u64(source.source_version().raw());
            }
        }
    }

    ProfileSubjectRecord {
        kind,
        fingerprint: encoder.finish(),
    }
}

const fn subject_kind_code(kind: CompilationProfileSubjectKind) -> u8 {
    match kind {
        CompilationProfileSubjectKind::Compilation => 0,
        CompilationProfileSubjectKind::SourceUnit => 1,
        CompilationProfileSubjectKind::SemanticUnit => 2,
        CompilationProfileSubjectKind::CodegenUnit => 3,
        CompilationProfileSubjectKind::Product => 4,
        CompilationProfileSubjectKind::Artifact => 5,
    }
}

fn encode_product(encoder: &mut StableSubjectEncoder, product: &bray_symbols::ProductIdentity) {
    encoder.write_str(product.package().as_str());
    encoder.write_str(product.name());
}

struct StableSubjectEncoder(StableDigestHasher);

impl StableSubjectEncoder {
    fn new() -> Self {
        Self(StableDigestHasher::new())
    }

    fn finish(&self) -> u64 {
        self.0.finish()
    }

    fn write_bytes(&mut self, bytes: &[u8]) {
        self.0.write(bytes);
    }

    fn write_str(&mut self, value: &str) {
        self.write_u64(u64::try_from(value.len()).unwrap_or(u64::MAX));
        self.write_bytes(value.as_bytes());
    }

    fn write_u8(&mut self, value: u8) {
        self.0.write_u8(value);
    }

    fn write_u16(&mut self, value: u16) {
        self.0.write_u16(value);
    }

    fn write_u32(&mut self, value: u32) {
        self.0.write_u32(value);
    }

    fn write_u64(&mut self, value: u64) {
        self.0.write_u64(value);
    }
}

#[cfg(test)]
mod tests {
    use super::profile_subject;
    use crate::fact::CompilationFactKey;
    use crate::profile::{CompilationProfileSubjectKind, ProfileQueryKind};

    #[test]
    fn subject_fingerprints_use_schema_stable_canonical_bytes() {
        let subject = profile_subject(
            ProfileQueryKind::SourceUnitSyntax,
            &CompilationFactKey::SourceUnitSyntax(bray_source::SourceId::new(7)),
        );

        assert_eq!(subject.kind, CompilationProfileSubjectKind::SourceUnit);
        assert_eq!(subject.fingerprint, 0xd7e3_3159_4597_ddc4);
    }
}
