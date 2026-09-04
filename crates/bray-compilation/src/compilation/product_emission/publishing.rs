use bray_codegen::{ArtifactContent, CodegenUnit};
use bray_emitter::{
    ArtifactContribution, ArtifactKind, ArtifactProducer, ArtifactPublisher, EmissionPlan,
    OutputSinkResolver,
};
use bray_runtime_interface::RootExecution;

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

pub(super) fn validate_executable_units(
    plan: &EmissionPlan,
    units: &[CodegenUnit],
) -> Result<(), ProductEmissionErrorKind> {
    let Some(host) = plan.request().executable_host() else {
        return Ok(());
    };

    let planned_units = units
        .iter()
        .filter(|unit| plan.backend_request(unit.key()).is_some())
        .collect::<Vec<_>>();

    let mut has_host = false;

    for unit in &planned_units {
        for mir in unit.mir_units() {
            if matches!(
                mir.kind(),
                bray_ir::MirUnitKind::ExecutableHost(candidate) if candidate == host
            ) {
                has_host = true;
            }
        }
    }

    if !has_host {
        return Err(ProductEmissionErrorKind::MissingExecutableHost);
    }

    for entry in host.entries() {
        let RootExecution::Asynchronous { frame } = entry.root() else {
            continue;
        };

        let has_root_frame = planned_units.iter().any(|unit| {
            unit.instances()
                .iter()
                .any(|instance| instance.protected_frame_identity() == Some(frame))
        });

        if !has_root_frame {
            return Err(ProductEmissionErrorKind::MissingRootFrame(frame));
        }
    }

    Ok(())
}

pub(super) fn publisher<'operation>(
    cancellation: &'operation CancellationToken,
    resolver: Option<&'operation dyn OutputSinkResolver>,
) -> ArtifactPublisher<'operation> {
    match resolver {
        Some(resolver) => ArtifactPublisher::with_sink_resolver(cancellation, resolver),
        None => ArtifactPublisher::new(cancellation),
    }
}
