use bray_symbols::DependencyStorageEffect;
use bray_symbols::{
    DependencyContractTemplateData, DependencyGuard, DependencyProjection, DependencyRequirement,
    DependencySubject, DependencySubjectRoot, SemanticValueStore,
};

use crate::{
    InterfaceDependencyGuard, InterfaceDependencyProjection, InterfaceDependencyRequirement,
    InterfaceDependencyRequirementValue, InterfaceDependencySubject,
    InterfaceDependencySubjectRoot, InterfaceSemantics,
};

use super::common::{convert_requirement_kind, resolve_exact, resolve_family};
use super::{InterfaceSemanticInternError, InterfaceSymbolResolver, InternState};

impl InternState<'_> {
    fn convert_captured_static(&self, root: &crate::InterfaceStaticReference, symbols: &impl InterfaceSymbolResolver)
        -> Result<bray_symbols::StaticReferenceSelection, Option<InterfaceSemanticInternError>>
    {
        let declaration = resolve_exact::<bray_symbols::StaticSymbolId>(symbols, &root.declaration).map_err(Some)?;
        let substitution = self.substitution_id(root.substitution).ok_or(None)?;

        let witnesses = root.selected_witnesses.iter().map(|witness| self.implementation_instance_id(*witness).ok_or(None))
            .collect::<Result<Vec<_>, _>>()?;

        let template = bray_symbols::StaticInstanceTemplateId::new(declaration);

        Ok(if root.closed {
            bray_symbols::StaticReferenceSelection::Closed(bray_symbols::StaticInstanceKey::new(template, substitution, witnesses, root.target.clone()))
        } else {
            bray_symbols::StaticReferenceSelection::open(template, substitution, witnesses, root.target.clone())
        })
    }

    pub(super) fn intern_dependency_contracts(
        &mut self,
        semantics: &InterfaceSemantics,
        store: &SemanticValueStore,
        symbols: &impl InterfaceSymbolResolver,
    ) -> Result<(), InterfaceSemanticInternError> {
        for (index, input) in semantics.dependency_contracts.iter().enumerate() {
            if self.dependency_contracts[index].is_some() {
                continue;
            }

            let Some(requirements) =
                self.convert_dependency_requirements(&input.requirements, symbols)?
            else {
                continue;
            };

            let mut contract = DependencyContractTemplateData::new(requirements);

            if let Some(requirements) = &input.abnormal_reports {
                let Some(requirements) =
                    self.convert_dependency_requirements(requirements, symbols)?
                else {
                    continue;
                };

                contract = contract.with_abnormal_reports(requirements);
            }

            let Some(effects) = self.convert_dependency_effects(input.effects.possible(), symbols)? else {
                continue;
            };

            let Some(normal_effects) = self.convert_dependency_effects(input.effects.normal(), symbols)? else {
                continue;
            };

            let Some(synchronous_cleanup) =
                self.convert_dependency_requirements(&input.synchronous_cleanup, symbols)?
            else {
                continue;
            };

            let observed_result = if let Some(term) = input.observed_result {
                let Some(term) = self.constant_term_id(term) else {
                    continue;
                };

                Some(term)
            } else {
                None
            };

            let captured_values = input.captured_values.iter().map(|outcome| {
                let guard = outcome.guard.map(|term| self.constant_term_id(term).ok_or(None)).transpose()?;

                let report = outcome.report.as_ref().map(|requirements|
                    self.convert_dependency_requirements(requirements, symbols).map_err(Some)?.map(bray_base::sorted_unique_shared_slice).ok_or(None))
                    .transpose()?;

                let bindings = outcome.bindings.iter().map(|(destination, value)| {
                let destination = self.convert_dependency_subject(destination, symbols).map_err(Some)?.ok_or(None)?;

                let value = value.try_map(&mut |input| self.constant_term_id(*input).ok_or(None), &mut |projection| {
                    self.convert_dependency_projections(std::slice::from_ref(projection), symbols).map_err(Some)?
                        .and_then(|projections| projections.into_iter().next()).ok_or(None)
                }, &mut |root| self.convert_captured_static(root, symbols), &mut |call| Ok(bray_symbols::CapturedCallable {
                    instance: self.callable_instance_id(call.instance).ok_or(None)?, signature: self.type_id(call.signature).ok_or(None)?,
                    receiver: call.receiver.map(|ty| self.type_id(ty).ok_or(None)).transpose()?,
                    requirement: call.requirement.map(|(ty, app)| Ok::<_, Option<InterfaceSemanticInternError>>(bray_symbols::ImplementationRequirementKey::new(
                        self.type_id(ty).ok_or(None)?, self.trait_application_id(app).ok_or(None)?))).transpose()?,
                    dispatch: call.dispatch.map(|w| self.implementation_instance_id(w).ok_or(None)).transpose()?,
                    witnesses: call.witnesses.iter().map(|w| self.implementation_instance_id(*w).ok_or(None)).collect::<Result<_, _>>()?,
                }), &mut |variant| resolve_exact(symbols, variant).map_err(Some))?;

                Ok((destination, value))
                }).collect::<Result<Vec<_>, Option<InterfaceSemanticInternError>>>()?;

                let owned_result = outcome.ownership.try_map(
                &mut |transfer| {
                    let term = self.constant_term_id(transfer.input()).ok_or(None)?;

                    let condition = transfer
                        .condition()
                        .map(|condition| self.constant_term_id(condition).ok_or(None))
                        .transpose()?;

                    let output = transfer
                        .output()
                        .iter()
                        .map(|projection| self.convert_constant_projection(projection, symbols))
                        .collect::<Result<Option<Vec<_>>, _>>()
                        .map_err(Some)?
                        .ok_or(None)?;

                    Ok(bray_symbols::DependencyOwnedResult::Transfer(
                        bray_symbols::DependencyOwnedTransfer::new(term, output)
                            .with_preservation(transfer.preservation().try_map_projections(&mut |projection| {
                                self.convert_constant_projection(projection, symbols)
                                    .map_err(Some)?
                                    .ok_or(None)
                            })?)
                            .with_condition(condition),
                    ))
                },
                &mut |condition| self.constant_term_id(*condition).map(Some).ok_or(None),
            )?;

                let carriers = outcome.carriers.iter().map(|carrier| {
                    let path = self.convert_dependency_projections(carrier.path(), symbols).map_err(Some)?.ok_or(None)?;

                    Ok(carrier.remap(self.constant_term_id(carrier.guarantee()).ok_or(None)?, path))
                }).collect::<Result<Vec<_>, Option<crate::InterfaceSemanticInternError>>>()?;

                let (references, stored_values) = self.convert_dependency_storage(&outcome.references, &outcome.stored_values, symbols)
                    .map_err(Some)?.ok_or(None)?;

                let inactive_payloads = outcome.inactive_payloads.iter().map(|payload|
                    self.convert_dependency_subject(payload, symbols).map_err(Some)?.ok_or(None))
                    .collect::<Result<Vec<_>, _>>()?;

                Ok(bray_symbols::CapturedValueOutcome::new(guard, bindings).with_carriers(carriers).with_report(report)
                    .with_ownership(owned_result).with_storage(references, stored_values)
                    .with_inactive_payloads(inactive_payloads))
            }).collect::<Result<Vec<_>, Option<InterfaceSemanticInternError>>>();

            let captured_values = match captured_values {
                Ok(values) => values,
                Err(None) => continue,
                Err(Some(error)) => return Err(error),
            };

            self.dependency_contracts[index] = Some(
                store.intern_dependency_contract_template(
                    contract
                        .with_captured_values(captured_values)
                        .with_observed_result(observed_result)
                        .with_effects(effects)
                        .with_normal_effects(normal_effects)
                        .with_synchronous_cleanup(synchronous_cleanup)
                        .with_normal_completion(input.normal_completion),
                )?,
            );
        }

        Ok(())
    }

    fn convert_dependency_storage(
        &self,
        references: &[bray_symbols::DependencyBorrowRelation<InterfaceDependencySubject, InterfaceDependencyProjection>],
        stored_values: &[bray_symbols::DependencyStoredValue<InterfaceDependencySubject>],
        symbols: &impl InterfaceSymbolResolver,
    ) -> Result<Option<(Vec<bray_symbols::DependencyBorrowRelation>, Vec<bray_symbols::DependencyStoredValue>)>, InterfaceSemanticInternError> {
        let mut origins = Vec::with_capacity(references.len());

        for relation in references {
            let entry = match relation.source_subject() {
                Some(entry) => {
                    let Some(entry) = self.convert_dependency_subject(entry, symbols)? else { return Ok(None); };

                    Some(entry)
                }
                None => None,
            };

            let Some(exit) = self.convert_dependency_subject(relation.exit(), symbols)? else { return Ok(None); };

            let Some(projections) = self.convert_dependency_projections(relation.target_projections(), symbols)? else { return Ok(None); };

            origins.push(relation.remap(entry, exit, projections));
        }

        let mut values = Vec::with_capacity(stored_values.len());

        for stored in stored_values {
            let Some(destination) = self.convert_dependency_subject(stored.destination().subject(), symbols)? else { return Ok(None); };

            let destination = bray_symbols::DependencyStoredValuePath::new(destination, stored.destination().dereferences());
            let mut sources = Vec::with_capacity(stored.sources().len());

            for source in stored.sources() {
                let Some(subject) = self.convert_dependency_subject(source.subject(), symbols)? else { return Ok(None); };

                sources.push(bray_symbols::DependencyStoredValuePath::new(subject, source.dereferences()));
            }

            values.push(bray_symbols::DependencyStoredValue::new(destination, sources, stored.is_complete()));
        }

        Ok(Some((origins, values)))
    }

    pub(super) fn convert_dependency_requirements(
        &self,
        inputs: &[InterfaceDependencyRequirement],
        symbols: &impl InterfaceSymbolResolver,
    ) -> Result<Option<Vec<DependencyRequirement>>, InterfaceSemanticInternError> {
        let mut values = Vec::with_capacity(inputs.len());

        for input in inputs {
            let value = match &input.value {
                InterfaceDependencyRequirementValue::CleanupDependencies { ty, requirements } => {
                    let Some(ty) = self.type_id(*ty) else {
                        return Ok(None);
                    };

                    let Some(requirements) =
                        self.convert_dependency_requirements(requirements, symbols)?
                    else {
                        return Ok(None);
                    };

                    DependencyRequirement::cleanup_dependencies(ty, requirements)
                }
                InterfaceDependencyRequirementValue::SynchronousCleanup(ty) => {
                    let Some(ty) = self.type_id(*ty) else {
                        return Ok(None);
                    };

                    DependencyRequirement::SynchronousCleanup(ty)
                }
                InterfaceDependencyRequirementValue::FinalizerBodyCleanup(ty) => {
                    let Some(ty) = self.type_id(*ty) else {
                        return Ok(None);
                    };

                    DependencyRequirement::FinalizerBodyCleanup(ty)
                }
                InterfaceDependencyRequirementValue::DefaultCleanup {
                    provider,
                    substitution,
                    contextual_self,
                } => {
                    let provider = resolve_family(symbols, provider)?;

                    let Some(substitution) = self.substitution_id(*substitution) else {
                        return Ok(None);
                    };

                    let contextual_self = match contextual_self {
                        Some((context, subject)) => {
                            let Some(subject) = self.type_id(*subject) else {
                                return Ok(None);
                            };

                            Some((resolve_family(symbols, context)?, subject))
                        }
                        None => None,
                    };

                    DependencyRequirement::DefaultCleanup {
                        provider,
                        substitution,
                        contextual_self,
                    }
                }
                InterfaceDependencyRequirementValue::Variable { depth, ordinal } => {
                    DependencyRequirement::variable(*depth, *ordinal)
                }
                InterfaceDependencyRequirementValue::FixedPoint {
                    definitions,
                    result,
                } => {
                    let mut converted = Vec::with_capacity(definitions.len());

                    for definition in definitions.iter() {
                        let Some(value) =
                            self.convert_dependency_requirements(definition, symbols)?
                        else {
                            return Ok(None);
                        };

                        converted.push(value);
                    }

                    let Some(result) = self.convert_dependency_requirements(result, symbols)?
                    else {
                        return Ok(None);
                    };

                    DependencyRequirement::fixed_point(converted, result)
                }
                InterfaceDependencyRequirementValue::ResultCall {
                    output,
                    callable,
                    requirement,
                    inputs,
                } => {
                    let Some(callable) = self.callable_instance_id(*callable) else {
                        return Ok(None);
                    };

                    let requirement = match requirement {
                        Some((subject, application)) => {
                            let (Some(subject), Some(application)) = (
                                self.type_id(*subject),
                                self.trait_application_id(*application),
                            ) else {
                                return Ok(None);
                            };

                            Some(bray_symbols::ImplementationRequirementKey::new(
                                subject,
                                application,
                            ))
                        }
                        None => None,
                    };

                    let Some(converted) = self.convert_dependency_call_inputs(inputs, symbols)?
                    else {
                        return Ok(None);
                    };

                    DependencyRequirement::result_call(callable, requirement, converted, *output)
                }
                InterfaceDependencyRequirementValue::Direct { subject, kind } => {
                    let Some(subject) = self.convert_dependency_subject(subject, symbols)? else {
                        return Ok(None);
                    };

                    DependencyRequirement::direct(subject, convert_requirement_kind(*kind))
                }
                InterfaceDependencyRequirementValue::Guarded {
                    guard,
                    requirements,
                } => {
                    let Some(guard) = self.convert_dependency_guard(guard, symbols)? else {
                        return Ok(None);
                    };

                    let Some(requirements) =
                        self.convert_dependency_requirements(requirements, symbols)?
                    else {
                        return Ok(None);
                    };

                    DependencyRequirement::guarded(guard, requirements)
                }
            };

            values.push(value);
        }

        Ok(Some(values))
    }

    fn convert_dependency_call_inputs(
        &self,
        inputs: &[crate::InterfaceDependencyCallInput],
        symbols: &impl InterfaceSymbolResolver,
    ) -> Result<Option<Vec<bray_symbols::DependencyCallInput>>, InterfaceSemanticInternError> {
        let mut converted = Vec::new();

        for input in inputs.iter() {
            let Some(root) = self.convert_dependency_subject(
                &InterfaceDependencySubject::new(input.root.clone(), []),
                symbols,
            )?
            else {
                return Ok(None);
            };

            let Some(values) = self.convert_dependency_requirements(&input.values, symbols)? else {
                return Ok(None);
            };

            let Some(storage) = self.convert_dependency_requirements(&input.storage, symbols)?
            else {
                return Ok(None);
            };

            converted.push(bray_symbols::DependencyCallInput::new(
                root.subject_root(),
                values,
                storage,
            ));
        }

        Ok(Some(converted))
    }

    pub(super) fn convert_dependency_subject(
        &self,
        input: &InterfaceDependencySubject,
        symbols: &impl InterfaceSymbolResolver,
    ) -> Result<Option<DependencySubject>, InterfaceSemanticInternError> {
        let root = match &input.root {
            InterfaceDependencySubjectRoot::Receiver => DependencySubjectRoot::Receiver,
            InterfaceDependencySubjectRoot::Parameter(ordinal) => {
                DependencySubjectRoot::Parameter(*ordinal)
            }
            InterfaceDependencySubjectRoot::Result => DependencySubjectRoot::Result,
            InterfaceDependencySubjectRoot::EvaluationStorage => {
                DependencySubjectRoot::EvaluationStorage
            }
            InterfaceDependencySubjectRoot::ScopedCapability(ordinal) => {
                DependencySubjectRoot::ScopedCapability(*ordinal)
            }
            InterfaceDependencySubjectRoot::ImplementationWitness(id) => {
                let Some(id) = self.implementation_instance_id(*id) else {
                    return Ok(None);
                };

                DependencySubjectRoot::ImplementationWitness(id)
            }
            InterfaceDependencySubjectRoot::ImplementationRequirement(ty, application) => {
                let (Some(ty), Some(application)) =
                    (self.type_id(*ty), self.trait_application_id(*application))
                else {
                    return Ok(None);
                };

                DependencySubjectRoot::ImplementationRequirement(
                    bray_symbols::ImplementationRequirementKey::new(ty, application),
                )
            }
            InterfaceDependencySubjectRoot::ProductStatic(reference) => {
                DependencySubjectRoot::ProductStatic(resolve_exact(symbols, reference)?)
            }
            InterfaceDependencySubjectRoot::ExactThreadStatic(reference) => {
                DependencySubjectRoot::ExactThreadStatic(resolve_exact(symbols, reference)?)
            }
            InterfaceDependencySubjectRoot::StaticInstance(reference, duration) => {
                let reference = match self.convert_captured_static(reference, symbols) {
                    Ok(reference) => reference,
                    Err(None) => return Ok(None),
                    Err(Some(error)) => return Err(error),
                };

                DependencySubjectRoot::StaticInstance(self.store.intern_static_reference(reference)?, *duration)
            }
        };

        let Some(projections) = self.convert_dependency_projections(&input.projections, symbols)?
        else {
            return Ok(None);
        };

        Ok(Some(DependencySubject::new(root, projections)))
    }

    fn convert_dependency_projections(
        &self,
        input: &[InterfaceDependencyProjection],
        symbols: &impl InterfaceSymbolResolver,
    ) -> Result<Option<Vec<DependencyProjection>>, InterfaceSemanticInternError> {
        let mut projections = Vec::with_capacity(input.len());

        for projection in input {
            let projection = match projection {
                InterfaceDependencyProjection::ProductField(field) => {
                    DependencyProjection::ProductField(resolve_exact(symbols, field)?)
                }
                InterfaceDependencyProjection::TupleElement(ordinal) => {
                    DependencyProjection::TupleElement(*ordinal)
                }
                InterfaceDependencyProjection::Element(id) => {
                    let Some(id) = self.constant_term_id(*id) else {
                        return Ok(None);
                    };

                    DependencyProjection::Element(id)
                }
                InterfaceDependencyProjection::NullableValue => DependencyProjection::NullableValue,
                InterfaceDependencyProjection::UnionPayloadField(field) => {
                    DependencyProjection::UnionPayloadField(resolve_exact(symbols, field)?)
                }
                InterfaceDependencyProjection::OwnedTarget => DependencyProjection::OwnedTarget,
                InterfaceDependencyProjection::UninitPayload => DependencyProjection::UninitPayload,
                InterfaceDependencyProjection::AnyElement => DependencyProjection::AnyElement,
                InterfaceDependencyProjection::AddressTarget => DependencyProjection::AddressTarget,
            };

            projections.push(projection);
        }

        Ok(Some(projections))
    }

    fn convert_dependency_effects(
        &self,
        input: &[bray_symbols::DependencyStorageEffect<
            InterfaceDependencySubject,
            InterfaceDependencyProjection,
            crate::InterfaceTypeId,
        >],
        symbols: &impl InterfaceSymbolResolver,
    ) -> Result<Option<Vec<bray_symbols::DependencyStorageEffect>>, InterfaceSemanticInternError>
    {

        let mut effects = Vec::with_capacity(input.len());

        for effect in input {
            effects.push(match effect {
                DependencyStorageEffect::UnboundedMutation { kind, access_type, ambient } => DependencyStorageEffect::UnboundedMutation {
                    kind: match kind.try_map_type(|ty| self.type_id(ty).ok_or(())) { Ok(kind) => kind, Err(()) => return Ok(None) },
                    access_type: match access_type.map(|ty| self.type_id(ty).ok_or(())).transpose() { Ok(ty) => ty, Err(()) => return Ok(None) },
                    ambient: *ambient,
                },
                DependencyStorageEffect::Atomic => DependencyStorageEffect::Atomic,
                DependencyStorageEffect::Ambient => DependencyStorageEffect::Ambient,
                DependencyStorageEffect::Unknown => DependencyStorageEffect::Unknown,
                DependencyStorageEffect::Escape => DependencyStorageEffect::Escape,
                DependencyStorageEffect::Direct { addressed, subject, kind, access_type } => {
                    let Some(subject) = self.convert_dependency_subject(subject, symbols)? else {
                        return Ok(None);
                    };

                    DependencyStorageEffect::Direct { addressed: *addressed,
                        subject,
                        kind: match kind.try_map_type(|ty| self.type_id(ty).ok_or(())) {
                            Ok(kind) => kind,
                            Err(()) => return Ok(None),
                        },
                        access_type: match access_type.map(|ty| self.type_id(ty).ok_or(())).transpose() {
                            Ok(ty) => ty,
                            Err(()) => return Ok(None),
                        },
                    }
                }
                DependencyStorageEffect::Indirect { addressed, reads,
                    reference,
                    projections,
                    kind,
                    access_type,
                } => {
                    let (Some(reference), Some(projections)) = (
                        self.convert_dependency_subject(reference, symbols)?,
                        self.convert_dependency_projections(projections, symbols)?,
                    ) else {
                        return Ok(None);
                    };

                    DependencyStorageEffect::Indirect { addressed: *addressed, reads: std::sync::Arc::clone(reads),
                        reference,
                        projections: projections.into(),
                        kind: match kind.try_map_type(|ty| self.type_id(ty).ok_or(())) {
                            Ok(kind) => kind,
                            Err(()) => return Ok(None),
                        },
                        access_type: match access_type.map(|ty| self.type_id(ty).ok_or(())).transpose() {
                            Ok(ty) => ty,
                            Err(()) => return Ok(None),
                        },
                    }
                }
            });
        }

        Ok(Some(effects))
    }

    pub(super) fn convert_dependency_guard(
        &self,
        input: &InterfaceDependencyGuard,
        symbols: &impl InterfaceSymbolResolver,
    ) -> Result<Option<DependencyGuard>, InterfaceSemanticInternError> {
        Ok(match input {
            InterfaceDependencyGuard::NullablePresent(subject) => {
                let Some(subject) = self.convert_dependency_subject(subject, symbols)? else {
                    return Ok(None);
                };

                Some(DependencyGuard::NullablePresent(subject))
            }
            InterfaceDependencyGuard::ActiveUnionVariant { subject, variant } => {
                let Some(subject) = self.convert_dependency_subject(subject, symbols)? else {
                    return Ok(None);
                };

                Some(DependencyGuard::ActiveUnionVariant {
                    subject,
                    variant: resolve_exact(symbols, variant)?,
                })
            }
        })
    }
}
