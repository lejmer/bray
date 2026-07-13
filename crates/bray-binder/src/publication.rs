use bray_bound_tree::{
    BoundBlockId, BoundCallableBodyId, BoundExpressionId, BoundUnit, BoundUnitBuildError,
    BoundUnitKey, BoundUnitKeyData, BoundUnitRoot,
};
use bray_diagnostics::DiagnosticResult;
use bray_symbols::AnonymousCallableSymbolId;

use crate::request::BinderRequestResult;
use crate::{BinderDependency, BoundUnitComputation};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BoundUnitAssemblyError {
    InvalidBoundUnit(BoundUnitBuildError),
}

impl From<BoundUnitBuildError> for BoundUnitAssemblyError {
    fn from(error: BoundUnitBuildError) -> Self {
        Self::InvalidBoundUnit(error)
    }
}

pub(crate) fn assemble_callable_body(
    request: BinderRequestResult,
    nested_units: Vec<BoundUnitKey>,
    root: BoundCallableBodyId,
) -> Result<BoundUnitComputation, BoundUnitAssemblyError> {
    assemble_bound_unit(request, nested_units, BoundUnitRoot::CallableBody(root))
}

pub(crate) fn assemble_anonymous_callable(
    request: BinderRequestResult,
    nested_units: Vec<BoundUnitKey>,
    callable: AnonymousCallableSymbolId,
    root: BoundCallableBodyId,
) -> Result<BoundUnitComputation, BoundUnitAssemblyError> {
    assemble_bound_unit(
        request,
        nested_units,
        BoundUnitRoot::AnonymousCallable {
            callable,
            body: root,
        },
    )
}

macro_rules! define_root_assembler {
    ($function:ident, $root_variant:ident, $root_type:ty) => {
        pub(crate) fn $function(
            request: BinderRequestResult,
            nested_units: Vec<BoundUnitKey>,
            root: $root_type,
        ) -> Result<BoundUnitComputation, BoundUnitAssemblyError> {
            assemble_bound_unit(request, nested_units, BoundUnitRoot::$root_variant(root))
        }
    };
}

define_root_assembler!(assemble_runtime_default, Expression, BoundExpressionId);
define_root_assembler!(assemble_constant_template, Expression, BoundExpressionId);
define_root_assembler!(assemble_predicate_definition, Expression, BoundExpressionId);
define_root_assembler!(assemble_constraint, ExpressionSequence, BoundBlockId);
define_root_assembler!(assemble_contract_clause, ExpressionSequence, BoundBlockId);

fn assemble_bound_unit(
    request: BinderRequestResult,
    nested_units: Vec<BoundUnitKey>,
    root: BoundUnitRoot,
) -> Result<BoundUnitComputation, BoundUnitAssemblyError> {
    let (unit, diagnostics, dependencies) = request.into_parts();
    let (key, tree, local_symbols, _) = unit.into_parts();

    let bound = BoundUnit::try_new(key, tree, local_symbols, nested_units, root)?;

    Ok(BoundUnitComputation::new(
        DiagnosticResult::new(bound, diagnostics),
        dependencies,
    ))
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
