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
) -> Result<ArtifactContribution, ProductEmissionErrorKind> {
    let planned = plan
        .published_artifacts()
        .find(|artifact| artifact.id().kind() == ArtifactKind::PackageImplementation)
        .ok_or(ProductEmissionErrorKind::Query(
            ProductQueryFailure::missing(
                ProductQueryContext::Product(plan.request().product_kind()),
                ProductDataKind::PackageImplementationArtifact,
            )
            .into(),
        ))?;

    let content = ArtifactContent::try_memory(artifact.shared_bytes())
        .map_err(ProductEmissionErrorKind::PackageImplementationContent)?;

    Ok(ArtifactContribution::new(
        planned.id().clone(),
        ArtifactProducer::PackageImplementation,
        content,
        None,
    ))
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
