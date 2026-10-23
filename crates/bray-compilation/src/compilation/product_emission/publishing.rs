use bray_codegen::ArtifactContent;
use bray_emitter::{
    ArtifactContribution, ArtifactKind, ArtifactProducer, ArtifactPublisher, EmissionPlan,
    OutputSinkResolver, PublicationValidator,
};

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
        .ok_or(ProductEmissionErrorKind::Query(
            ProductQueryFailure::missing(
                ProductQueryContext::Product(plan.request().product_kind()),
                ProductDataKind::PackageImplementationArtifact,
            )
            .into(),
        ))?;

    artifact.verify_all().expect(
        "compiler-produced package implementation must pass complete publication authentication",
    );

    let content = ArtifactContent::try_memory(artifact.shared_bytes().map_err(|error| {
        package_implementation_error(
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

pub(super) fn package_implementation_error(
    error: bray_package_interface::PackageImplementationArtifactBuildError,
) -> ProductEmissionErrorKind {
    use bray_package_interface::{
        InterfaceCompressionFailure, InterfaceValidationError,
        PackageImplementationArtifactBuildError,
    };

    match &error {
        PackageImplementationArtifactBuildError::InvalidBody(validation)
        | PackageImplementationArtifactBuildError::InvalidArtifact(validation)
            if matches!(
                validation,
                InterfaceValidationError::Read { .. }
                    | InterfaceValidationError::AllocationUnavailable { .. }
                    | InterfaceValidationError::ResourceLimitExceeded { .. }
                    | InterfaceValidationError::Compression {
                        cause: InterfaceCompressionFailure::EncoderInitialization
                            | InterfaceCompressionFailure::Encoding
                            | InterfaceCompressionFailure::DecoderInitialization
                            | InterfaceCompressionFailure::Decoding
                            | InterfaceCompressionFailure::WindowSizeExceeded { .. },
                        ..
                    }
            ) =>
        {
            ProductEmissionErrorKind::PackageImplementation(error)
        }
        _ => panic!(
            "compiler-produced package implementation violates its checked export contract: {error:?}"
        ),
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

        let super::ProductEmissionErrorKind::PackageImplementation(
            PackageImplementationArtifactBuildError::InvalidBody(actual),
        ) = super::package_implementation_error(failure)
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
        super::package_implementation_error(PackageImplementationArtifactBuildError::InvalidBody(
            InterfaceValidationError::Malformed {
                context: InterfaceValidationContext::Section(
                    InterfaceSectionTag::SemanticRecordDirectory,
                ),
                cause: InterfaceMalformedCause::InvalidValue {
                    field: InterfaceValidationField::Template,
                },
            },
        ));
    }
}
