use std::hash::{Hash, Hasher};

use bray_bound_tree::{
    BoundBlockId, BoundCallableBodyId, BoundExpressionId, BoundUnit, BoundUnitBuildError,
    BoundUnitKey, BoundUnitKeyData, BoundUnitRoot,
};
use bray_diagnostics::DiagnosticResult;
use bray_symbols::{AnonymousCallableSymbolId, CallableExecution};

use crate::binder::BinderOutput;
use crate::{BinderDependency, BoundUnitComputation};

/// A typed failure while assembling completed binder structures into a bound unit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BoundUnitAssemblyError {
    /// The completed bound tree and local symbols violate a bound-unit contract.
    InvalidBoundUnit(BoundUnitBuildError),
}

impl Hash for BoundUnitAssemblyError {
    fn hash<H: Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);

        match self {
            Self::InvalidBoundUnit(error) => hash_bound_unit_error(*error, state),
        }
    }
}

fn hash_bound_unit_error<H: Hasher>(error: BoundUnitBuildError, state: &mut H) {
    std::mem::discriminant(&error).hash(state);

    match error {
        BoundUnitBuildError::MissingContractParameter { parameter } => parameter.hash(state),
        BoundUnitBuildError::ContractParameterCountMismatch { expected, actual } => {
            expected.hash(state);
            actual.hash(state);
        }
        BoundUnitBuildError::InvalidContractParameter { index, parameter } => {
            index.hash(state);
            parameter.hash(state);
        }
        BoundUnitBuildError::RootKindMismatch | BoundUnitBuildError::LocalSymbolRegionMismatch => {}
        BoundUnitBuildError::MissingRoot { unit, kind } => {
            unit.hash(state);
            kind.hash(state);
        }
        BoundUnitBuildError::AnonymousCallableRegionMismatch { expected, actual } => {
            expected.hash(state);
            actual.hash(state);
        }
        BoundUnitBuildError::MissingAnonymousCallable { callable } => callable.hash(state),
        BoundUnitBuildError::InvalidNestedUnit { index }
        | BoundUnitBuildError::NonCanonicalNestedUnits { index } => index.hash(state),
    }
}

impl From<BoundUnitBuildError> for BoundUnitAssemblyError {
    fn from(error: BoundUnitBuildError) -> Self {
        Self::InvalidBoundUnit(error)
    }
}

pub(crate) fn assemble_callable_body(
    output: BinderOutput,
    nested_units: Vec<BoundUnitKey>,
    execution: CallableExecution,
    root: BoundCallableBodyId,
) -> Result<BoundUnitComputation, BoundUnitAssemblyError> {
    assemble_bound_unit(
        output,
        nested_units,
        BoundUnitRoot::CallableBody {
            execution,
            body: root,
        },
    )
}

pub(crate) fn assemble_anonymous_callable(
    output: BinderOutput,
    nested_units: Vec<BoundUnitKey>,
    callable: AnonymousCallableSymbolId,
    execution: CallableExecution,
    root: BoundCallableBodyId,
) -> Result<BoundUnitComputation, BoundUnitAssemblyError> {
    assemble_bound_unit(
        output,
        nested_units,
        BoundUnitRoot::AnonymousCallable {
            callable,
            execution,
            body: root,
        },
    )
}

macro_rules! define_root_assembler {
    ($function:ident, $root_variant:ident, $root_type:ty) => {
        pub(crate) fn $function(
            output: BinderOutput,
            nested_units: Vec<BoundUnitKey>,
            root: $root_type,
        ) -> Result<BoundUnitComputation, BoundUnitAssemblyError> {
            assemble_bound_unit(output, nested_units, BoundUnitRoot::$root_variant(root))
        }
    };
}

define_root_assembler!(assemble_runtime_default, Expression, BoundExpressionId);
define_root_assembler!(assemble_constant_template, Expression, BoundExpressionId);
define_root_assembler!(assemble_embedded_constant, Expression, BoundExpressionId);
define_root_assembler!(assemble_predicate_definition, Expression, BoundExpressionId);
define_root_assembler!(assemble_constraint, ExpressionSequence, BoundBlockId);
define_root_assembler!(assemble_contract_clause, ExpressionSequence, BoundBlockId);
define_root_assembler!(assemble_target_gate, Expression, BoundExpressionId);

fn assemble_bound_unit(
    output: BinderOutput,
    nested_units: Vec<BoundUnitKey>,
    root: BoundUnitRoot,
) -> Result<BoundUnitComputation, BoundUnitAssemblyError> {
    let (unit, diagnostics, dependencies, contract_inputs) = output.into_parts();

    let (key, tree, local_symbols, _) = unit.into_parts();

    let bound = BoundUnit::try_new(key, tree, local_symbols, nested_units, root)?;

    let bound = match contract_inputs {
        Some((signature, parameters)) => bound.try_with_contract_inputs(signature, parameters)?,
        None => bound,
    };

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
