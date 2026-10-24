use bray_codegen::ArtifactContent;
use bray_emitter::{
    ArtifactContribution, ArtifactKind, ArtifactProducer, ArtifactPublisher, EmissionPlan,
    OutputSinkResolver, PublicationValidator,
};
use bray_package_interface::PackageImplementationArtifactBuildError;

use super::ProductEmissionErrorKind;
use crate::compilation::{ProductDataKind, ProductQueryContext, ProductQueryFailure};
use crate::fact::CancellationToken;

pub(super) fn package_implementation_contribution(
    plan: &EmissionPlan,
    artifact: bray_package_interface::PackageImplementationArtifact,
    kind: ArtifactKind,
) -> Result<ArtifactContribution, ProductEmissionErrorKind> {
    let planned = plan
        .published_artifacts()
        .find(|artifact| artifact.id().kind() == kind)
        .ok_or_else(|| {
            ProductEmissionErrorKind::Query(
                ProductQueryFailure::missing(
                    ProductQueryContext::Product(plan.request().product_kind()),
                    ProductDataKind::PackageImplementationArtifact,
                )
                .into(),
            )
        })?;

    artifact.verify_all().map_err(|cause| match cause {
        bray_package_interface::PackageNativeArtifactError::Validation(cause) => {
            ProductEmissionErrorKind::from(
                PackageImplementationArtifactBuildError::InvalidArtifact(cause),
            )
        }
        bray_package_interface::PackageNativeArtifactError::Index(
            bray_native_artifact::NativeIndexError::SizeLimitExceeded,
        ) => ProductEmissionErrorKind::NativeIndexSizeLimitExceeded,
        bray_package_interface::PackageNativeArtifactError::Index(
            bray_native_artifact::NativeIndexError::Read { path, kind },
        ) => ProductEmissionErrorKind::NativeRead { path, kind },
        cause => {
            panic!("compiler-produced package implementation authentication failed: {cause:?}")
        }
    })?;

    let content = ArtifactContent::try_memory(artifact.shared_bytes().map_err(|error| {
        ProductEmissionErrorKind::from(
            bray_package_interface::PackageImplementationArtifactBuildError::InvalidArtifact(error),
        )
    })?)
    .map_err(ProductEmissionErrorKind::PackageImplementationContent)?;

    Ok(ArtifactContribution::new(
        planned.id().clone(),
        ArtifactProducer::PackageImplementation,
        content,
        None,
    ))
}

impl From<PackageImplementationArtifactBuildError> for ProductEmissionErrorKind {
    #[track_caller]
    fn from(error: PackageImplementationArtifactBuildError) -> Self {
        use bray_package_interface::{InterfaceCompressionFailure, InterfaceValidationError};

        match error {
            PackageImplementationArtifactBuildError::InvalidBody(cause)
            | PackageImplementationArtifactBuildError::InvalidArtifact(cause)
                if matches!(
                    &cause,
                    InterfaceValidationError::Read { .. }
                        | InterfaceValidationError::ResourceLimitExceeded { .. }
                        | InterfaceValidationError::AllocationUnavailable { .. }
                        | InterfaceValidationError::Compression {
                            cause: InterfaceCompressionFailure::EncoderInitialization
                                | InterfaceCompressionFailure::EncoderConfiguration
                                | InterfaceCompressionFailure::Encoding
                                | InterfaceCompressionFailure::DecoderInitialization
                                | InterfaceCompressionFailure::DecoderConfiguration
                                | InterfaceCompressionFailure::Decoding,
                            ..
                        }
                ) =>
            {
                Self::PackageImplementationValidation(cause)
            }
            _ => panic!(
                "compiler-produced package implementation violates its checked export contract: {error:?}"
            ),
        }
    }
}

pub(super) fn test_catalog_contribution(
    plan: &EmissionPlan,
    catalog: &[u8],
) -> Result<ArtifactContribution, ProductEmissionErrorKind> {
    let planned = plan
        .published_artifacts()
        .find(|artifact| artifact.id().kind() == ArtifactKind::TestCatalog)
        .ok_or(ProductEmissionErrorKind::MissingTestCatalogArtifact)?;

    let content = ArtifactContent::try_memory(catalog)
        .map_err(ProductEmissionErrorKind::TestCatalogContent)?;

    Ok(ArtifactContribution::new(
        planned.id().clone(),
        ArtifactProducer::TestCatalog,
        content,
        None,
    ))
}

pub(super) fn publisher<'operation>(
    cancellation: &'operation CancellationToken,
    resolver: Option<&'operation dyn OutputSinkResolver>,
    validation: Option<&'operation dyn PublicationValidator>,
) -> ArtifactPublisher<'operation> {
    let publisher = match resolver {
        Some(resolver) => ArtifactPublisher::with_sink_resolver(cancellation, resolver),
        None => ArtifactPublisher::new(cancellation),
    };

    match validation {
        Some(validation) => publisher.with_publication_validation(validation),
        None => publisher,
    }
}

#[cfg(test)]
mod tests {
    use bray_package_interface::{
        InterfaceMalformedCause, InterfaceSectionTag, InterfaceValidationContext,
        InterfaceValidationError, InterfaceValidationField,
        PackageImplementationArtifactBuildError,
    };

    #[test]
    fn implementation_publication_retains_operational_allocation_context() {
        let error = InterfaceValidationError::AllocationUnavailable {
            context: InterfaceValidationContext::Section(
                InterfaceSectionTag::SemanticRecordDirectory,
            ),
            field: InterfaceValidationField::Template,
            requested: 42,
        };

        let failure = PackageImplementationArtifactBuildError::InvalidBody(error.clone());

        let super::ProductEmissionErrorKind::PackageImplementationValidation(actual) =
            super::ProductEmissionErrorKind::from(failure)
        else {
            panic!("publication must retain its operational allocation failure");
        };

        assert_eq!(actual, error);
    }

    #[test]
    #[should_panic(
        expected = "compiler-produced package implementation violates its checked export contract"
    )]
    fn malformed_compiler_template_fails_at_publication() {
        let _ = super::ProductEmissionErrorKind::from(
            PackageImplementationArtifactBuildError::InvalidBody(
                InterfaceValidationError::Malformed {
                    context: InterfaceValidationContext::Section(
                        InterfaceSectionTag::SemanticRecordDirectory,
                    ),
                    cause: InterfaceMalformedCause::InvalidValue {
                        field: InterfaceValidationField::Template,
                    },
                },
            ),
        );
    }
}
