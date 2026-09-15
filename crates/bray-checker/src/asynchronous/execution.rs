use std::collections::BTreeSet;

use bray_bound_tree::{AnyBoundNodeId, BoundCallableTarget};
use bray_compiler_known::RepresentationRole;
use bray_diagnostics::DiagnosticResult;
use bray_symbols::{
    CallableExecution, DeclaredStorageShape, GenericArgument, TypeAssociatedLifecycleSlot,
    TypeData, TypeId,
};

use super::cleanup::CleanupShapeResolver;
use crate::execution_guarantees::{ExecutionDependency, ExecutionProperty};
use crate::{CheckerQueryError, CheckerRequestContext, CheckerUnitView};

#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) enum ExecutionCleanupMode {
    Disposal,
    Admission,
}

pub(crate) fn execution_cleanup_dependencies<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    ty: TypeId,
    property: ExecutionProperty,
    mode: ExecutionCleanupMode,
    node: AnyBoundNodeId,
) -> Result<DiagnosticResult<Option<Vec<ExecutionDependency>>>, CheckerQueryError<C::UpstreamError>>
{
    let mut resolver = CleanupShapeResolver::new(request);
    let mut pending = vec![ty];
    let mut visited = BTreeSet::new();
    let mut dependencies = Vec::new();
    let mut valid = true;

    while let Some(ty) = pending.pop() {
        if request.is_cancelled() {
            return Err(CheckerQueryError::Cancelled);
        }

        if !visited.insert(ty) {
            continue;
        }

        if visited.len() > 1024 {
            valid = false;
            break;
        }

        let data = request.semantic_values().type_data(ty);

        match data.as_ref() {
            TypeData::Borrow { .. }
            | TypeData::Callable(_)
            | TypeData::TraitView(_)
            | TypeData::Slice(_)
            | TypeData::FlexibleArray(_) => {}
            TypeData::Tuple(elements) => {
                if mode == ExecutionCleanupMode::Disposal {
                    pending.extend(elements.iter().copied());
                }
            }
            TypeData::Array { element, .. } | TypeData::Nullable(element) => {
                if mode == ExecutionCleanupMode::Disposal {
                    pending.push(*element);
                }
            }
            TypeData::Named {
                definition,
                substitution,
            } => {
                let role = crate::representation::type_representation(request, ty);

                match role {
                    Some(RepresentationRole::Future | RepresentationRole::Task) => valid = false,
                    Some(RepresentationRole::String | RepresentationRole::PanicReport) => {
                        valid &= mode == ExecutionCleanupMode::Admission
                            || property == ExecutionProperty::Total
                    }
                    Some(
                        RepresentationRole::Result
                        | RepresentationRole::RunResult
                        | RepresentationRole::ConversionError,
                    ) => {
                        if mode == ExecutionCleanupMode::Admission {
                            continue;
                        }

                        let substitution = request
                            .semantic_values()
                            .generic_substitution_data(*substitution);

                        pending.extend(substitution.bindings().iter().filter_map(|binding| {
                            match binding.argument() {
                                GenericArgument::Type(ty) => Some(ty),
                                GenericArgument::Constant(_) => None,
                            }
                        }));
                    }
                    Some(_) => {}
                    None => {
                        for slot in [
                            TypeAssociatedLifecycleSlot::Finalizer,
                            TypeAssociatedLifecycleSlot::Destructor,
                        ] {
                            let selected = request.context().lifecycle_callable(ty, slot)?;

                            // The aggregate owns diagnostics beyond the selected query result.
                            resolver
                                .diagnostics
                                .add_range(selected.diagnostics().clone());

                            valid &= !selected.diagnostics().has_errors();

                            if let Some((callable, signature)) = selected.value() {
                                let callable_type = request
                                    .semantic_values()
                                    .type_data(signature.callable_type());

                                let TypeData::Callable(callable_type) = callable_type.as_ref()
                                else {
                                    valid = false;
                                    continue;
                                };

                                let unit = crate::representation::type_representation(
                                    request,
                                    signature.result(),
                                ) == Some(RepresentationRole::Unit);

                                if mode == ExecutionCleanupMode::Disposal
                                    && slot == TypeAssociatedLifecycleSlot::Finalizer
                                {
                                    valid &= property == ExecutionProperty::Total;
                                }

                                valid &= unit
                                    && callable_type.execution() == CallableExecution::Synchronous;

                                dependencies.push(ExecutionDependency {
                                    target: BoundCallableTarget::Declaration(*callable),
                                    property: if mode == ExecutionCleanupMode::Admission {
                                        ExecutionProperty::Total
                                    } else {
                                        property
                                    },
                                    node,
                                });
                            }
                        }

                        // Moved children retain their own admission. Only disposal traverses them.
                        if mode == ExecutionCleanupMode::Admission {
                            continue;
                        }

                        let representation = request.declared_type_representation(*definition)?;

                        // Retain representation diagnostics after this type has been traversed.
                        resolver
                            .diagnostics
                            .add_range(representation.diagnostics().clone());

                        valid &= !representation.value().is_recovered();

                        let members = match representation.value().storage() {
                            DeclaredStorageShape::Structure(members) => {
                                members.iter().map(|member| member.ty()).collect::<Vec<_>>()
                            }
                            DeclaredStorageShape::Union(variants) => variants
                                .iter()
                                .flat_map(|variant| variant.members())
                                .map(|member| member.ty())
                                .collect(),
                        };

                        for member in members {
                            match resolver.member_type(member, *substitution)? {
                                Some(ty) => pending.push(ty),
                                None => valid = false,
                            }
                        }
                    }
                }
            }
            TypeData::OwnedIndirection { .. }
            | TypeData::Error
            | TypeData::TypeParameter(_)
            | TypeData::ContextualSelf(_)
            | TypeData::TypeValuedMemberProjection { .. }
            | TypeData::Generator(_) => valid = false,
        }
    }

    Ok(DiagnosticResult::new(
        valid.then_some(dependencies),
        resolver.diagnostics,
    ))
}
