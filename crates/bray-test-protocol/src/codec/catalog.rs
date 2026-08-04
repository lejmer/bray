use std::hash::Hasher;

use bray_base::StableDigestHasher;
use bray_source::{SourceId, SourceSpan, SourceVersion, TextRange, TextSize};
use bray_symbols::{
    CallableExecution, ModulePathKey, PackageIdentity, ProductIdentity, SymbolName,
    TestExecutionConstraint, TestResultShape,
};

use crate::{
    TestCatalog, TestDeclarationPath, TestEntryMetadata, TestErrorTypeIdentity, TestIdentity,
    TestSourceAnchor,
};

use super::support::{Decoder, Encoder, MAX_COLLECTION_ITEMS, TestProtocolError};

const CATALOG_MAGIC: &[u8; 8] = b"BRAYTSTC";

/// Stable digest of one canonical encoded test catalog.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TestCatalogDigest([u8; 32]);

impl TestCatalogDigest {
    /// Returns the exact 256-bit digest bytes.
    pub const fn bytes(self) -> [u8; 32] {
        self.0
    }
}

/// Encodes one validated product catalog in canonical entry order.
pub fn encode_test_catalog(
    catalog: &TestCatalog,
) -> Result<(Vec<u8>, TestCatalogDigest), TestProtocolError> {
    let mut encoder = Encoder::new(CATALOG_MAGIC);

    encode_product(&mut encoder, catalog.product())?;
    encoder.length(catalog.entries().len())?;

    for entry in catalog.entries() {
        encode_entry(&mut encoder, entry)?;
    }

    let bytes = encoder.finish()?;
    let mut digest = StableDigestHasher::new();

    digest.write(&bytes);

    Ok((bytes, TestCatalogDigest(digest.finalize())))
}

/// Decodes and validates one bounded canonical product catalog.
pub fn decode_test_catalog(
    bytes: &[u8],
) -> Result<(TestCatalog, TestCatalogDigest), TestProtocolError> {
    let mut decoder = Decoder::new(bytes, CATALOG_MAGIC)?;
    let product = decode_product(&mut decoder)?;
    let entry_count = decoder.length()?;

    if entry_count > MAX_COLLECTION_ITEMS {
        return Err(TestProtocolError::ResourceLimit);
    }

    let entries = (0..entry_count)
        .map(|_| decode_entry(&mut decoder, &product))
        .collect::<Result<Vec<_>, _>>()?;

    decoder.finish()?;

    let catalog =
        TestCatalog::try_new(product, entries).map_err(|_| TestProtocolError::Malformed)?;

    let mut digest = StableDigestHasher::new();

    digest.write(bytes);

    Ok((catalog, TestCatalogDigest(digest.finalize())))
}

fn encode_entry(encoder: &mut Encoder, entry: &TestEntryMetadata) -> Result<(), TestProtocolError> {
    let path = entry.identity().declaration();
    let segments = path.module().segments().collect::<Vec<_>>();

    encoder.length(segments.len())?;

    for segment in segments {
        encoder.string(segment)?;
    }

    encoder.string(path.name().as_str())?;

    let source = entry.source();
    let span = source.span();

    encoder.u32(span.source_id().raw());
    encoder.u32(span.start().bytes());
    encoder.u32(span.end().bytes());
    encoder.u64(source.version().raw());

    encoder.u8(match entry.execution() {
        CallableExecution::Synchronous => 0,
        CallableExecution::Asynchronous => 1,
    });

    encoder.u8(match entry.constraint() {
        TestExecutionConstraint::Parallel => 0,
        TestExecutionConstraint::Serial => 1,
    });

    encoder.u8(match entry.result() {
        TestResultShape::Unit => 0,
        TestResultShape::Recoverable => 1,
    });

    match entry.error_type() {
        Some(error_type) => {
            encoder.u8(1);
            encoder.string(error_type.as_str())?;
        }
        None => encoder.u8(0),
    }

    Ok(())
}

fn decode_entry(
    decoder: &mut Decoder<'_>,
    product: &ProductIdentity,
) -> Result<TestEntryMetadata, TestProtocolError> {
    let segment_count = decoder.length()?;

    if segment_count == 0 {
        return Err(TestProtocolError::Malformed);
    }

    let segments = (0..segment_count)
        .map(|_| decoder.string().map(str::to_owned))
        .collect::<Result<Vec<_>, _>>()?;

    let module = ModulePathKey::try_new(segments).ok_or(TestProtocolError::Malformed)?;
    let name = SymbolName::try_new(decoder.string()?).ok_or(TestProtocolError::Malformed)?;
    let source = SourceId::stored(decoder.u32()?).ok_or(TestProtocolError::Malformed)?;
    let start = TextSize::new(decoder.u32()?);
    let end = TextSize::new(decoder.u32()?);

    if start > end {
        return Err(TestProtocolError::Malformed);
    }

    let version = SourceVersion::new(decoder.u64()?);

    let execution = match decoder.u8()? {
        0 => CallableExecution::Synchronous,
        1 => CallableExecution::Asynchronous,
        _ => return Err(TestProtocolError::Malformed),
    };

    let constraint = match decoder.u8()? {
        0 => TestExecutionConstraint::Parallel,
        1 => TestExecutionConstraint::Serial,
        _ => return Err(TestProtocolError::Malformed),
    };

    let result = match decoder.u8()? {
        0 => TestResultShape::Unit,
        1 => TestResultShape::Recoverable,
        _ => return Err(TestProtocolError::Malformed),
    };

    let error_type = match decoder.u8()? {
        0 => None,
        1 => Some(
            TestErrorTypeIdentity::try_new(decoder.string()?)
                .ok_or(TestProtocolError::Malformed)?,
        ),
        _ => return Err(TestProtocolError::Malformed),
    };

    let declaration = TestDeclarationPath::new(module, name);
    let identity = TestIdentity::new(product.clone(), declaration);

    let source =
        TestSourceAnchor::new(SourceSpan::new(source, TextRange::new(start, end)), version);

    Ok(TestEntryMetadata::new(
        identity,
        source,
        execution,
        constraint,
        result,
        error_type,
    ))
}

fn encode_product(
    encoder: &mut Encoder,
    product: &ProductIdentity,
) -> Result<(), TestProtocolError> {
    encoder.string(product.package().as_str())?;

    encoder.string(product.name())
}

fn decode_product(decoder: &mut Decoder<'_>) -> Result<ProductIdentity, TestProtocolError> {
    let package =
        PackageIdentity::try_new(decoder.string()?).ok_or(TestProtocolError::Malformed)?;

    ProductIdentity::try_new(package, decoder.string()?).ok_or(TestProtocolError::Malformed)
}

#[cfg(test)]
mod tests {
    use bray_source::{SourceId, SourceSpan, SourceVersion, TextRange, TextSize};
    use bray_symbols::{
        CallableExecution, ModulePathKey, PackageIdentity, ProductIdentity, SymbolName,
        TestExecutionConstraint, TestResultShape,
    };

    use crate::{
        TestCatalog, TestDeclarationPath, TestEntryMetadata, TestErrorTypeIdentity, TestIdentity,
        TestSourceAnchor,
    };

    #[test]
    fn catalogs_round_trip_with_a_stable_digest() {
        let product = product();

        let catalog = TestCatalog::try_new(product.clone(), [entry(product)])
            .unwrap_or_else(|error| panic!("test catalog must be valid: {error:?}"));

        let (bytes, digest) = super::encode_test_catalog(&catalog)
            .unwrap_or_else(|error| panic!("test catalog must encode: {error:?}"));

        let (decoded, decoded_digest) = super::decode_test_catalog(&bytes)
            .unwrap_or_else(|error| panic!("test catalog must decode: {error:?}"));

        assert_eq!(decoded, catalog);
        assert_eq!(decoded_digest, digest);
    }

    fn entry(product: ProductIdentity) -> TestEntryMetadata {
        let module = ModulePathKey::try_new(["example", "tests"])
            .unwrap_or_else(|| panic!("test module path must be valid"));

        let name = SymbolName::try_new("runs")
            .unwrap_or_else(|| panic!("test function name must be valid"));

        let identity = TestIdentity::new(product, TestDeclarationPath::new(module, name));

        let source = TestSourceAnchor::new(
            SourceSpan::new(
                SourceId::new(2),
                TextRange::new(TextSize::new(3), TextSize::new(9)),
            ),
            SourceVersion::new(4),
        );

        TestEntryMetadata::new(
            identity,
            source,
            CallableExecution::Asynchronous,
            TestExecutionConstraint::Serial,
            TestResultShape::Recoverable,
            TestErrorTypeIdentity::try_new("example.tests.Error"),
        )
    }

    fn product() -> ProductIdentity {
        let package = PackageIdentity::try_new("example.tests")
            .unwrap_or_else(|| panic!("test package identity must be valid"));

        ProductIdentity::try_new(package, "tests")
            .unwrap_or_else(|| panic!("test product identity must be valid"))
    }
}
