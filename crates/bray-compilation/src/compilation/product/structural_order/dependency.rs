use super::encoding::OrderKey;
use super::{
    encoding::{sequence, term},
    values::OrderEncoder,
};
use crate::compilation::CodegenPreparationError;
use bray_symbols::{
    DependencyGuard, DependencyProjection, DependencyRequirement, DependencyRequirementKind,
    DependencySubject, DependencySubjectRoot,
};

impl OrderEncoder<'_, '_> {
    pub(super) fn dependencies(
        &self,
        requirements: &[DependencyRequirement],
    ) -> Result<OrderKey, CodegenPreparationError> {
        let mut keys = requirements
            .iter()
            .map(|requirement| self.dependency(requirement))
            .collect::<Result<Vec<_>, _>>()?;

        keys.sort_unstable();

        Ok(sequence(keys))
    }

    fn dependency(
        &self,
        requirement: &DependencyRequirement,
    ) -> Result<OrderKey, CodegenPreparationError> {
        Ok(match requirement {
            DependencyRequirement::Variable { depth, ordinal } => term(
                "variable",
                [
                    depth.to_be_bytes().to_vec().into(),
                    ordinal.raw().to_be_bytes().to_vec().into(),
                ],
            ),
            DependencyRequirement::FixedPoint {
                definitions,
                result,
            } => term(
                "fixed_point",
                [
                    sequence(
                        definitions
                            .iter()
                            .map(|set| self.dependencies(set))
                            .collect::<Result<Vec<_>, _>>()?,
                    ),
                    self.dependencies(result)?,
                ],
            ),
            DependencyRequirement::ResultCall {
                callable,
                requirement,
                inputs,
            } => {
                let values = self.compilation.semantic_value_store()?;
                let callable = values.callable_instance_data(*callable);

                let requirement = requirement
                    .as_ref()
                    .map(|requirement| {
                        Ok::<_, CodegenPreparationError>(sequence([
                            self.ty(requirement.subject())?,
                            self.trait_application(requirement.trait_application())?,
                        ]))
                    })
                    .transpose()?;

                let inputs = inputs
                    .iter()
                    .map(|input| {
                        Ok(sequence([
                            self.subject(&DependencySubject::root(input.root()))?,
                            self.dependencies(input.values())?,
                            self.dependencies(input.storage())?,
                        ]))
                    })
                    .collect::<Result<Vec<_>, CodegenPreparationError>>()?;

                term(
                    "result_call",
                    [
                        self.symbol(callable.definition().symbol())?,
                        self.substitution(callable.substitution())?,
                        sequence(requirement),
                        sequence(inputs),
                    ],
                )
            }
            DependencyRequirement::Direct { subject, kind } => {
                term("direct", [self.subject(subject)?, requirement_kind(*kind)])
            }
            DependencyRequirement::Guarded(guarded) => {
                let guard = match guarded.guard() {
                    DependencyGuard::NullablePresent(subject) => {
                        term("nullable_present", [self.subject(subject)?])
                    }
                    DependencyGuard::ActiveUnionVariant { subject, variant } => term(
                        "active_union_variant",
                        [self.subject(subject)?, self.symbol((*variant).into())?],
                    ),
                };

                term(
                    "guarded",
                    [guard, self.dependencies(guarded.requirements())?],
                )
            }
        })
    }

    fn subject(&self, subject: &DependencySubject) -> Result<OrderKey, CodegenPreparationError> {
        let root = match subject.subject_root() {
            DependencySubjectRoot::Receiver => term("receiver", []),
            DependencySubjectRoot::Result => term("result", []),
            DependencySubjectRoot::EvaluationStorage => term("evaluation_storage", []),
            DependencySubjectRoot::Parameter(ordinal) => {
                term("parameter", [ordinal.raw().to_be_bytes().to_vec().into()])
            }
            DependencySubjectRoot::ScopedCapability(ordinal) => term(
                "scoped_capability",
                [ordinal.raw().to_be_bytes().to_vec().into()],
            ),
            DependencySubjectRoot::ImplementationWitness(id) => {
                term("implementation_witness", [self.implementation(id)?])
            }
            DependencySubjectRoot::ProductStatic(id) => {
                term("product_static", [self.symbol(id.into())?])
            }
            DependencySubjectRoot::ExactThreadStatic(id) => {
                term("exact_thread_static", [self.symbol(id.into())?])
            }
        };

        let projections = subject
            .projections()
            .iter()
            .map(|projection| {
                Ok(match projection {
                    DependencyProjection::ProductField(id) => {
                        term("product_field", [self.symbol((*id).into())?])
                    }
                    DependencyProjection::TupleElement(ordinal) => term(
                        "tuple_element",
                        [ordinal.raw().to_be_bytes().to_vec().into()],
                    ),
                    DependencyProjection::Element(index) => {
                        term("element", [self.constant(*index)?])
                    }
                    DependencyProjection::NullableValue => term("nullable_value", []),
                    DependencyProjection::UnionPayloadField(id) => {
                        term("union_payload_field", [self.symbol((*id).into())?])
                    }
                    DependencyProjection::OwnedTarget => term("owned_target", []),
                })
            })
            .collect::<Result<Vec<_>, CodegenPreparationError>>()?;

        Ok(sequence([root, sequence(projections)]))
    }
}

fn requirement_kind(kind: DependencyRequirementKind) -> OrderKey {
    match kind {
        DependencyRequirementKind::StorageAlive => term("storage_alive", []),
        DependencyRequirementKind::ValueDependencies => term("value_dependencies", []),
        DependencyRequirementKind::StorageInitialized => term("storage_initialized", []),
        DependencyRequirementKind::BorrowCapabilityActive(kind) => term(
            "borrow_capability_active",
            [kind.as_str().as_bytes().to_vec().into()],
        ),
        DependencyRequirementKind::ExclusiveMutationAuthority => {
            term("exclusive_mutation_authority", [])
        }
        DependencyRequirementKind::ScopedCapabilityLive => term("scoped_capability_live", []),
        DependencyRequirementKind::LifecycleObligation(kind) => {
            term("lifecycle_obligation", [super::callable::lifecycle(kind)])
        }
    }
}
