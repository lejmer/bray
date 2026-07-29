use std::sync::Arc;

use bray_base::shared_slice;

use super::{CodegenReachability, CodegenUnit, CodegenUnitBuildError};

/// Partitions a closed reachable graph using the initial one-instance-per-unit policy.
///
/// The revision belongs to the caller's selected partition policy and participates in every
/// resulting unit key.
pub fn partition_codegen_units(
    partition_revision: u32,
    reachability: &CodegenReachability,
) -> Result<Arc<[CodegenUnit]>, CodegenUnitBuildError> {
    reachability
        .instances()
        .iter()
        .cloned()
        .map(|instance| CodegenUnit::try_from_instances(partition_revision, [instance]))
        .collect::<Result<Vec<_>, _>>()
        .map(shared_slice)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bray_testing::{test_mir_unit, test_mir_unit_with_declaration};

    use super::partition_codegen_units;
    use crate::{CodegenInstance, CodegenReachabilityBuilder};

    #[test]
    fn partition_membership_is_stable_for_reversed_root_order() {
        let first = CodegenInstance::non_generic(test_mir_unit(4));
        let second = CodegenInstance::non_generic(test_mir_unit_with_declaration(8, 1));

        let forward = partitions([first.clone(), second.clone()]);
        let reversed = partitions([second, first]);

        assert_eq!(forward, reversed);
        assert_eq!(forward.len(), 2);
    }

    fn partitions(
        instances: impl IntoIterator<Item = CodegenInstance>,
    ) -> Arc<[crate::CodegenUnit]> {
        let instances: Vec<_> = instances.into_iter().collect();

        let Ok(mut builder) = CodegenReachabilityBuilder::try_new(
            instances.iter().map(|instance| instance.key().clone()),
        ) else {
            panic!("test roots must validate");
        };

        let _ = builder.take_frontier();

        for instance in instances {
            if let Err(error) = builder.push_instance(instance) {
                panic!("test instance must publish: {error:?}");
            }
        }

        let Ok(graph) = builder.finish() else {
            panic!("test graph must close");
        };

        match partition_codegen_units(7, &graph) {
            Ok(units) => units,
            Err(error) => panic!("test partitions must validate: {error:?}"),
        }
    }
}
