use bray_binder::{BindingQueryContext, BindingQueryError, BindingQueryResult};
use bray_bound_tree::{
    BoundDependencyContract, BoundDependencyGuard, BoundDependencyRequirement,
    BoundDependencySubject, StorageAccessId, StorageIdentity, StoragePlan, StorageProjection,
};
use bray_symbols::{
    DependencyContractTemplateData, DependencyContractTemplateId, DependencyGuard,
    DependencyProjection, DependencyRequirement, DependencySubject, DependencySubjectRoot,
    SymbolOrdinal,
};

use crate::compilation::binder::CompilationBindingContext;

pub(in crate::compilation::binder::symbol) fn portable_dependency_contract(
    context: &CompilationBindingContext<'_>,
    storage: &StoragePlan,
    contract: &BoundDependencyContract,
) -> BindingQueryResult<DependencyContractTemplateId> {
    let mut requirements = Vec::new();

    for requirement in contract.requirements() {
        if let Some(requirement) = portable_requirement(context, storage, requirement)? {
            requirements.push(requirement);
        }
    }

    context
        .semantic_values()
        .intern_dependency_contract_template(DependencyContractTemplateData::new(requirements))
        .map_err(|_| BindingQueryError::DependencyUnavailable)
}

fn portable_requirement(
    context: &CompilationBindingContext<'_>,
    storage: &StoragePlan,
    requirement: &BoundDependencyRequirement,
) -> BindingQueryResult<Option<DependencyRequirement>> {
    match requirement {
        BoundDependencyRequirement::Direct { subject, kind } => {
            let Some(subject) = portable_bound_subject(context, storage, *subject)? else {
                return Ok(None);
            };

            Ok(Some(DependencyRequirement::direct(
                subject.subject,
                (*kind).into(),
            )))
        }
        BoundDependencyRequirement::Guarded(guarded) => {
            let Some(guard) = portable_guard(context, storage, guarded.guard())? else {
                return Ok(None);
            };

            let mut requirements = Vec::new();

            for requirement in guarded.requirements() {
                let Some(requirement) = portable_requirement(context, storage, requirement)? else {
                    return Ok(None);
                };

                requirements.push(requirement);
            }

            Ok(Some(DependencyRequirement::guarded(guard, requirements)))
        }
    }
}

fn portable_guard(
    context: &CompilationBindingContext<'_>,
    storage: &StoragePlan,
    guard: BoundDependencyGuard,
) -> BindingQueryResult<Option<DependencyGuard>> {
    match guard {
        BoundDependencyGuard::NullablePresent(access) => Ok(portable_guard_subject(
            context,
            storage,
            access,
            |projection| matches!(projection, StorageProjection::NullableValue),
        )?
        .map(|subject| DependencyGuard::NullablePresent(subject.subject))),
        BoundDependencyGuard::ActiveUnionVariant { access, variant } => {
            Ok(
                portable_guard_subject(context, storage, access, |projection| {
                    matches!(
                        projection,
                        StorageProjection::ActiveUnionPayloadField {
                            variant: candidate,
                            ..
                        } if *candidate == variant
                    )
                })?
                .map(|subject| DependencyGuard::ActiveUnionVariant {
                    subject: subject.subject,
                    variant,
                }),
            )
        }
        BoundDependencyGuard::BorrowCapabilityActive(_)
        | BoundDependencyGuard::ScopedCapabilityLive(_) => Ok(None),
    }
}

fn portable_bound_subject(
    context: &CompilationBindingContext<'_>,
    storage: &StoragePlan,
    subject: BoundDependencySubject,
) -> BindingQueryResult<Option<PortableSubject>> {
    match subject {
        BoundDependencySubject::Storage(identity) => {
            portable_storage_identity(context, storage, identity)
        }
        BoundDependencySubject::StorageAccess(access) => portable_subject(context, storage, access),
        BoundDependencySubject::BorrowCapability(capability) => {
            let capability = storage
                .borrow_capability(capability)
                .ok_or(BindingQueryError::DependencyUnavailable)?;

            portable_subject(context, storage, capability.access())
        }
        BoundDependencySubject::ImplementationWitness(witness) => Ok(Some(PortableSubject::root(
            DependencySubjectRoot::ImplementationWitness(witness),
        ))),
        BoundDependencySubject::ScopedCapability(_)
        | BoundDependencySubject::LifecycleObligation(_) => Ok(None),
    }
}

fn portable_subject(
    context: &CompilationBindingContext<'_>,
    storage: &StoragePlan,
    access: StorageAccessId,
) -> BindingQueryResult<Option<PortableSubject>> {
    let projection_count = storage
        .resolved_projections(access)
        .ok_or(BindingQueryError::DependencyUnavailable)?
        .len();

    portable_subject_with_projection_count(context, storage, access, projection_count)
}

fn portable_guard_subject(
    context: &CompilationBindingContext<'_>,
    storage: &StoragePlan,
    access: StorageAccessId,
    is_guard_projection: impl FnOnce(&StorageProjection) -> bool,
) -> BindingQueryResult<Option<PortableSubject>> {
    let projections = storage
        .resolved_projections(access)
        .ok_or(BindingQueryError::DependencyUnavailable)?;

    let projection_count = if projections.last().is_some_and(is_guard_projection) {
        projections.len().saturating_sub(1)
    } else {
        projections.len()
    };

    portable_subject_with_projection_count(context, storage, access, projection_count)
}

fn portable_subject_with_projection_count(
    context: &CompilationBindingContext<'_>,
    storage: &StoragePlan,
    access: StorageAccessId,
    projection_count: usize,
) -> BindingQueryResult<Option<PortableSubject>> {
    let Some(identity) = storage.root_identity(access) else {
        return Err(BindingQueryError::DependencyUnavailable);
    };

    let Some(mut subject) = portable_storage_identity(context, storage, identity)? else {
        return Ok(None);
    };

    for projection in storage
        .resolved_projections(access)
        .ok_or(BindingQueryError::DependencyUnavailable)?
        .iter()
        .take(projection_count)
    {
        match *projection {
            StorageProjection::ProductField(field) => {
                subject.append_projection(DependencyProjection::ProductField(field));
            }
            StorageProjection::TupleElement(ordinal) => {
                subject.append_projection(DependencyProjection::TupleElement(ordinal));
            }
            StorageProjection::ActiveUnionPayloadField { field, .. } => {
                subject.append_projection(DependencyProjection::UnionPayloadField(field));
            }
            StorageProjection::NullableValue => {
                subject.append_projection(DependencyProjection::NullableValue);
            }
            StorageProjection::OwnedTarget => {
                subject.append_projection(DependencyProjection::OwnedTarget);
            }
            StorageProjection::ElementFromStart(_)
            | StorageProjection::ElementFromEnd(_)
            | StorageProjection::Element(_)
            | StorageProjection::SliceRange { .. } => return Ok(None),
        }
    }

    Ok(Some(subject))
}

fn portable_storage_identity(
    context: &CompilationBindingContext<'_>,
    storage: &StoragePlan,
    identity: bray_bound_tree::StorageIdentityId,
) -> BindingQueryResult<Option<PortableSubject>> {
    let identity = storage
        .identity(identity)
        .ok_or(BindingQueryError::DependencyUnavailable)?;

    let root = match identity {
        StorageIdentity::Parameter(parameter) => {
            let parameter = context
                .symbols()
                .callable_parameter(parameter)
                .ok_or(BindingQueryError::DependencyUnavailable)?;

            DependencySubjectRoot::Parameter(SymbolOrdinal::new(parameter.ordinal()))
        }
        StorageIdentity::PredicateParameter(parameter) => {
            let parameter = context
                .symbols()
                .predicate_parameter(parameter)
                .ok_or(BindingQueryError::DependencyUnavailable)?;

            DependencySubjectRoot::Parameter(SymbolOrdinal::new(parameter.ordinal()))
        }
        StorageIdentity::Receiver(_) => DependencySubjectRoot::Receiver,
        StorageIdentity::Result(_) | StorageIdentity::PostconditionResult(_) => {
            DependencySubjectRoot::Result
        }
        StorageIdentity::LocalOwned(_)
        | StorageIdentity::Alternative { .. }
        | StorageIdentity::AnonymousParameter(_)
        | StorageIdentity::Temporary(_)
        | StorageIdentity::CustomIndexBorrow(_)
        | StorageIdentity::IterationCursor(_)
        | StorageIdentity::IterationElement(_)
        | StorageIdentity::Allocation(_)
        | StorageIdentity::CompilerCreated(_)
        | StorageIdentity::Error(_) => return Ok(None),
    };

    Ok(Some(PortableSubject::root(root)))
}

struct PortableSubject {
    subject: DependencySubject,
}

impl PortableSubject {
    fn root(root: DependencySubjectRoot) -> Self {
        Self {
            subject: DependencySubject::root(root),
        }
    }

    fn append_projection(&mut self, projection: DependencyProjection) {
        self.subject = DependencySubject::new(
            self.subject.subject_root(),
            self.subject
                .projections()
                .iter()
                .copied()
                .chain([projection]),
        );
    }
}
