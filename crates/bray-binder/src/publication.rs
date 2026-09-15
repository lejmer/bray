use bray_bound_tree::{BoundUnit, BoundUnitKey, BoundUnitKeyData, BoundUnitRoot};
use bray_diagnostics::DiagnosticResult;

use crate::binder::BinderOutput;
use crate::{BinderDependency, BoundUnitComputation};

pub(crate) fn assemble_bound_unit(
    output: BinderOutput,
    root: BoundUnitRoot,
) -> BoundUnitComputation {
    let (unit, diagnostics, dependencies) = output.into_parts();

    let (key, tree, local_symbols, _) = unit.into_parts();

    let nested_units = direct_nested_units(&key, &dependencies);
    let bound = BoundUnit::new(key, tree, local_symbols, nested_units, root);

    BoundUnitComputation::new(DiagnosticResult::new(bound, diagnostics), dependencies)
}

pub(crate) fn direct_nested_units(
    enclosing: &BoundUnitKey,
    dependencies: &[BinderDependency],
) -> Vec<BoundUnitKey> {
    dependencies
        .iter()
        .filter_map(|dependency| {
            let BinderDependency::Unit(nested) = dependency else {
                return None;
            };

            let BoundUnitKeyData::AnonymousCallable(anonymous) = nested.data() else {
                return None;
            };

            if anonymous.enclosing() != enclosing {
                return None;
            }

            // The bound unit and query dependency set both retain this Arc-backed key.
            Some(nested.clone())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::direct_nested_units;
    use crate::BinderDependency;
    use crate::unit::test_support::fixture;

    #[test]
    fn nested_units_include_only_direct_anonymous_dependencies() {
        let fixture = fixture();

        let first = bray_bound_tree::BoundUnitKey::anonymous_callable(
            fixture.key.clone(),
            bray_bound_tree::BoundSourceAnchor::new(fixture.second, fixture.version),
        );

        let second = bray_bound_tree::BoundUnitKey::anonymous_callable(
            fixture.key.clone(),
            bray_bound_tree::BoundSourceAnchor::new(fixture.foreign, fixture.version),
        );

        let nested = direct_nested_units(
            &fixture.key,
            &[
                BinderDependency::Unit(first.clone()),
                BinderDependency::Unit(second.clone()),
            ],
        );

        assert_eq!(nested, [first, second]);
    }
}
