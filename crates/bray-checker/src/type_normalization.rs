use std::collections::{BTreeMap, BTreeSet};

use bray_diagnostics::DiagnosticBag;
use bray_symbols::{
    CallableParameterData, CallableSignature, CallableTypeData, GenericArgument,
    GenericSubstitutionData, GenericSubstitutionId, TraitApplicationData, TraitApplicationId,
    TypeData, TypeId,
};

use crate::{CheckerQueryError, CheckerRequestContext};

pub fn normalize_type_valued_members<C>(
    request: &C,
    ty: TypeId,
    diagnostics: &mut DiagnosticBag,
) -> Result<TypeId, CheckerQueryError<C::UpstreamError>>
where
    C: CheckerRequestContext + ?Sized,
{
    let mut normalizer = TypeNormalizer::new(request, None);
    let ty = normalizer.normalize_type(ty)?;

    diagnostics.extend(normalizer.take_diagnostics());

    Ok(ty)
}

/// Resolves implementation Self and type-valued members in an instantiated callable signature.
pub fn normalize_callable_signature<C>(
    request: &C,
    signature: CallableSignature,
    substitution: Option<GenericSubstitutionId>,
    diagnostics: &mut DiagnosticBag,
) -> Result<CallableSignature, CheckerQueryError<C::UpstreamError>>
where
    C: CheckerRequestContext + ?Sized,
{
    let mut normalizer = TypeNormalizer::new(request, substitution);
    let signature = signature.try_map_types(|ty| normalizer.normalize_type(ty))?;

    diagnostics.extend(normalizer.take_diagnostics());

    Ok(signature)
}

/// Resolves types on demand within one immutable checking and substitution context.
pub struct TypeNormalizer<'request, C>
where
    C: CheckerRequestContext + ?Sized,
{
    request: &'request C,
    diagnostics: DiagnosticBag,
    substitution: Option<GenericSubstitutionId>,
    equalities: BTreeMap<TypeId, TypeId>,
    normalized: BTreeMap<TypeId, TypeId>,
    active: BTreeSet<TypeId>,
}

impl<'request, C> TypeNormalizer<'request, C>
where
    C: CheckerRequestContext + ?Sized,
{
    /// Creates a normalizer that shares results across types in the same context.
    pub fn new(request: &'request C, substitution: Option<GenericSubstitutionId>) -> Self {
        let equalities = request.type_equalities();

        let representatives =
            equality_representatives(request.semantic_values(), equalities.value());

        Self {
            request,
            // The normalizer retains the shared diagnostics of its checked premises.
            diagnostics: equalities.diagnostics().clone(),
            substitution,
            equalities: representatives,
            normalized: BTreeMap::new(),
            active: BTreeSet::new(),
        }
    }

    /// Takes diagnostics produced since the previous call, retaining normalized type results.
    pub fn take_diagnostics(&mut self) -> DiagnosticBag {
        std::mem::take(&mut self.diagnostics)
    }

    /// Resolves type members and closed extents in an already substituted type.
    pub fn normalize_type(
        &mut self,
        ty: TypeId,
    ) -> Result<TypeId, CheckerQueryError<C::UpstreamError>> {
        if let Some(normalized) = self.normalized.get(&ty).copied() {
            return Ok(normalized);
        }

        if self.active.contains(&ty) {
            return Ok(ty);
        }

        // Normalize children first so generic nesting does not consume the host call stack.
        let mut pending = vec![(ty, false)];

        while let Some((current, ready)) = pending.pop() {
            if self.normalized.contains_key(&current) {
                continue;
            }

            if ready {
                self.active.remove(&current);
                self.normalize_type_node(current)?;
                continue;
            }

            if !self.active.insert(current) {
                continue;
            }

            pending.push((current, true));

            if let Some(canonical) = self.equalities.get(&current).copied() {
                pending.push((canonical, false));
                continue;
            }

            let data = self.request.semantic_values().type_data(current);

            match data.as_ref() {
                TypeData::Named { substitution, .. } => {
                    self.append_type_arguments(*substitution, &mut pending)
                }
                TypeData::TypeValuedMemberProjection {
                    subject,
                    application,
                    ..
                } => {
                    let application = self
                        .request
                        .semantic_values()
                        .trait_application_data(*application);

                    self.append_type_arguments(application.substitution(), &mut pending);
                    pending.push((*subject, false));
                }
                TypeData::Tuple(elements) => {
                    pending.extend(elements.iter().rev().map(|ty| (*ty, false)))
                }
                TypeData::Array { element, .. }
                | TypeData::FlexibleArray(element)
                | TypeData::Slice(element)
                | TypeData::Generator(element)
                | TypeData::Nullable(element)
                | TypeData::Borrow {
                    target: element, ..
                } => pending.push((*element, false)),
                TypeData::OwnedIndirection { storage, target } => {
                    pending.push((*target, false));
                    pending.push((*storage, false));
                }
                TypeData::TraitView(application) => {
                    let application = self
                        .request
                        .semantic_values()
                        .trait_application_data(*application);

                    self.append_type_arguments(application.substitution(), &mut pending);
                }
                TypeData::Callable(callable) => {
                    pending.push((callable.result(), false));

                    pending.extend(
                        callable
                            .parameters()
                            .iter()
                            .rev()
                            .map(|parameter| (parameter.ty(), false)),
                    );
                }
                TypeData::Error
                | TypeData::ExactInteger { .. }
                | TypeData::TypeParameter(_)
                | TypeData::ContextualSelf(_) => {}
            }
        }

        Ok(*self
            .normalized
            .get(&ty)
            .expect("type normalization finishes its requested root"))
    }

    fn append_type_arguments(
        &self,
        substitution: GenericSubstitutionId,
        pending: &mut Vec<(TypeId, bool)>,
    ) {
        let data = self
            .request
            .semantic_values()
            .generic_substitution_data(substitution);

        pending.extend(data.bindings().iter().rev().filter_map(
            |binding| match binding.argument() {
                GenericArgument::Type(ty) => Some((ty, false)),
                GenericArgument::Constant(_) => None,
            },
        ));
    }

    fn normalize_type_node(
        &mut self,
        ty: TypeId,
    ) -> Result<TypeId, CheckerQueryError<C::UpstreamError>> {
        assert!(
            self.active.insert(ty),
            "staged type normalization enters one node at a time"
        );

        if let Some(canonical) = self.equalities.get(&ty).copied()
            && canonical != ty
        {
            let normalized = self.normalize_type(canonical)?;

            self.active.remove(&ty);
            self.normalized.insert(ty, normalized);

            return Ok(normalized);
        }

        let data = self.request.semantic_values().type_data(ty);

        let normalized = match data.as_ref() {
            TypeData::ContextualSelf(bray_symbols::SelfTypeContext::Implementation(
                implementation,
            )) => {
                let subject = self.request.implementation_subject_type(*implementation)?;

                self.diagnostics
                    .extend(subject.diagnostics().iter().cloned());

                let subject = match self.substitution {
                    Some(substitution) => self
                        .request
                        .semantic_values()
                        .substitute_type(*subject.value(), substitution).unwrap_or_else(|error| panic!("The canonical semantic value store rejected a construction or lookup operation. in normalize_type: {error:?}")),
                    None => *subject.value(),
                };

                self.normalize_type(subject)?
            }
            TypeData::ContextualSelf(bray_symbols::SelfTypeContext::NamedType(definition)) => {
                let subject = self
                    .request
                    .semantic_values()
                    .intern_open_named_type(self.request.symbols(), *definition)
                    .expect("named Self has a valid generic identity")
                    .expect("named Self retains its declaration");

                let subject = match self.substitution {
                    Some(substitution) => self
                        .request
                        .semantic_values()
                        .substitute_type(subject, substitution)
                        .expect("selected Self substitution is valid"),
                    None => subject,
                };

                self.normalize_type(subject)?
            }
            TypeData::ExactInteger { .. }
            | TypeData::Error
            | TypeData::TypeParameter(_)
            | TypeData::ContextualSelf(_) => ty,
            TypeData::Named {
                definition,
                substitution,
            } => {
                let substitution = self.normalize_substitution(*substitution)?;

                self.intern(TypeData::Named {
                    definition: *definition,
                    substitution,
                })?
            }
            TypeData::TypeValuedMemberProjection {
                subject,
                application,
                member,
            } => {
                let subject = self.normalize_type(*subject)?;
                let application = self.normalize_trait_application(*application)?;

                let result =
                    self.request
                        .selected_type_valued_member(subject, application, *member)?;

                self.diagnostics
                    .extend(result.diagnostics().iter().cloned());

                match *result.value() {
                    Some(selected) if selected != ty => self.normalize_type(selected)?,
                    Some(_) | None => self.intern(TypeData::TypeValuedMemberProjection {
                        subject,
                        application,
                        member: *member,
                    })?,
                }
            }
            TypeData::Tuple(elements) => {
                let elements = elements
                    .iter()
                    .copied()
                    .map(|element| self.normalize_type(element))
                    .collect::<Result<Vec<_>, _>>()?;

                self.intern(TypeData::tuple(elements))?
            }
            TypeData::Array { element, length } => {
                let element = self.normalize_type(*element)?;

                let length = self.normalize_term(*length)?;
                let length = self.request.normalize_array_length(length)?;
                let recovered = length.diagnostics().has_errors();

                self.diagnostics
                    .extend(length.diagnostics().iter().cloned());

                if recovered {
                    self.intern(TypeData::Error)?
                } else {
                    self.intern(TypeData::Array {
                        element,
                        length: *length.value(),
                    })?
                }
            }
            TypeData::FlexibleArray(element) => {
                let element = self.normalize_type(*element)?;

                self.intern(TypeData::FlexibleArray(element))?
            }
            TypeData::Slice(element) => {
                let element = self.normalize_type(*element)?;

                self.intern(TypeData::Slice(element))?
            }
            TypeData::Generator(element) => {
                let element = self.normalize_type(*element)?;

                self.intern(TypeData::Generator(element))?
            }
            TypeData::Nullable(target) => {
                let target = self.normalize_type(*target)?;

                self.intern(TypeData::Nullable(target))?
            }
            TypeData::Borrow { kind, target } => {
                let target = self.normalize_type(*target)?;

                self.intern(TypeData::Borrow {
                    kind: *kind,
                    target,
                })?
            }
            TypeData::TraitView(application) => {
                let application = self.normalize_trait_application(*application)?;

                self.intern(TypeData::TraitView(application))?
            }
            TypeData::OwnedIndirection { storage, target } => {
                let storage = self.normalize_type(*storage)?;
                let target = self.normalize_type(*target)?;

                self.intern(TypeData::OwnedIndirection { storage, target })?
            }
            TypeData::Callable(callable) => self.normalize_callable_type(callable)?,
        };

        let normalized = if normalized != ty && self.equalities.contains_key(&normalized) {
            self.normalize_type(normalized)?
        } else {
            normalized
        };

        self.active.remove(&ty);
        self.normalized.insert(ty, normalized);

        Ok(normalized)
    }

    fn normalize_callable_type(
        &mut self,
        callable: &CallableTypeData,
    ) -> Result<TypeId, CheckerQueryError<C::UpstreamError>> {
        let parameters = callable
            .parameters()
            .iter()
            .map(|parameter| {
                self.normalize_type(parameter.ty()).map(|ty| {
                    CallableParameterData::new(
                        parameter.name().clone(),
                        parameter.position(),
                        parameter.mode(),
                        ty,
                    )
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        let result = self.normalize_type(callable.result())?;

        let phases = if self.equalities.is_empty() {
            // Phase behavior is immutable Arc-backed data shared by the rebuilt type.
            callable.phase_behaviors().clone()
        } else {
            self.prepare_equality_types()?;

            self.request
                .semantic_values()
                .replace_phase_types(callable.phase_behaviors(), &self.normalized)
                .expect("checked type aliases preserve phase contracts")
        };

        let callable = CallableTypeData::new(
            parameters,
            result,
            callable.constness(),
            callable.trust(),
            callable.abi(),
            phases.dependency_contracts(),
        )
        .with_variadic(callable.is_variadic())
        .with_phase_behaviors(phases);

        self.intern(TypeData::Callable(callable))
    }

    pub(crate) fn normalize_term(
        &mut self,
        term: bray_symbols::ConstantTermId,
    ) -> Result<bray_symbols::ConstantTermId, CheckerQueryError<C::UpstreamError>> {
        if self.equalities.is_empty() {
            return Ok(term);
        }

        self.prepare_equality_types()?;

        Ok(self
            .request
            .semantic_values()
            .replace_term_types(term, &self.normalized)
            .expect("checked type aliases preserve constant terms"))
    }

    /// Normalizes checked trait and predicate premises, retaining equality declarations.
    pub fn normalize_constraint(
        &mut self,
        constraint: bray_symbols::CheckedConstraint,
    ) -> Result<bray_symbols::CheckedConstraint, CheckerQueryError<C::UpstreamError>> {
        if self.equalities.is_empty() {
            return Ok(constraint);
        }

        Ok(match constraint.kind() {
            bray_symbols::CheckedConstraintKind::TraitSatisfaction {
                subject,
                application,
            } => bray_symbols::CheckedConstraint::trait_satisfaction(
                constraint.ordinal(),
                self.normalize_type(subject)?,
                self.normalize_trait_application(application)?,
            ),
            bray_symbols::CheckedConstraintKind::Predicate(predicate) => {
                bray_symbols::CheckedConstraint::new(
                    constraint.ordinal(),
                    self.normalize_predicate(predicate)?,
                )
            }
            bray_symbols::CheckedConstraintKind::TypeEquality { .. } => constraint,
        })
    }

    /// Normalizes a checked predicate under the same type aliases as source expressions.
    pub(crate) fn normalize_predicate(
        &mut self,
        predicate: bray_symbols::PredicateSemanticSummary,
    ) -> Result<bray_symbols::PredicateSemanticSummary, CheckerQueryError<C::UpstreamError>> {
        if self.equalities.is_empty() {
            return Ok(predicate);
        }

        self.prepare_equality_types()?;

        Ok(self
            .request
            .semantic_values()
            .replace_predicate_types(predicate, &self.normalized)
            .expect("checked type aliases preserve predicate subjects and domain"))
    }

    fn prepare_equality_types(&mut self) -> Result<(), CheckerQueryError<C::UpstreamError>> {
        let types = self.equalities.keys().copied().collect::<Vec<_>>();

        for ty in types {
            self.normalize_type(ty)?;
        }

        Ok(())
    }

    /// Normalizes type arguments of one checked trait application.
    pub(crate) fn normalize_trait_application(
        &mut self,
        application: TraitApplicationId,
    ) -> Result<TraitApplicationId, CheckerQueryError<C::UpstreamError>> {
        let data = self
            .request
            .semantic_values()
            .trait_application_data(application);

        let substitution = self.normalize_substitution(data.substitution())?;

        Ok(self.request
            .semantic_values()
            .intern_trait_application(TraitApplicationData::new(data.definition(), substitution)).unwrap_or_else(|error| panic!("normalize_application must satisfy its checked construction contract: {error:?}")))
    }

    fn normalize_substitution(
        &mut self,
        substitution: GenericSubstitutionId,
    ) -> Result<GenericSubstitutionId, CheckerQueryError<C::UpstreamError>> {
        let data = self
            .request
            .semantic_values()
            .generic_substitution_data(substitution);

        let parameters = data.bindings().iter().map(|binding| binding.parameter());

        let arguments = data
            .bindings()
            .iter()
            .map(|binding| match binding.argument() {
                GenericArgument::Type(ty) => self.normalize_type(ty).map(GenericArgument::Type),
                GenericArgument::Constant(term) => {
                    self.normalize_term(term).map(GenericArgument::Constant)
                }
            })
            .collect::<Result<Vec<_>, _>>()?;

        let normalized = GenericSubstitutionData::try_new(data.owner(), parameters, arguments).unwrap_or_else(|error| panic!("normalize_substitution must satisfy its checked construction contract: {error:?}"));

        Ok(self.request
            .semantic_values()
            .intern_generic_substitution(normalized).unwrap_or_else(|error| panic!("normalize_substitution must satisfy its checked construction contract: {error:?}")))
    }

    fn intern(&self, data: TypeData) -> Result<TypeId, CheckerQueryError<C::UpstreamError>> {
        Ok(self
            .request
            .semantic_values()
            .intern_type(data)
            .unwrap_or_else(|error| {
                panic!("intern must satisfy its checked construction contract: {error:?}")
            }))
    }
}

fn equality_representatives(
    values: &bray_symbols::SemanticValueStore,
    equalities: &[(TypeId, TypeId)],
) -> BTreeMap<TypeId, TypeId> {
    let mut representatives = BTreeMap::new();

    for &(left, right) in equalities {
        let left = *representatives.entry(left).or_insert(left);
        let right = *representatives.entry(right).or_insert(right);

        if left == right {
            continue;
        }

        let rank = |ty| {
            let category = match values.type_data(ty).as_ref() {
                TypeData::TypeValuedMemberProjection { .. } => 2,
                TypeData::TypeParameter(_) | TypeData::ContextualSelf(_) => 1,
                _ => 0,
            };

            (category, ty)
        };

        let (parent, child) = if rank(left) < rank(right) {
            (left, right)
        } else {
            (right, left)
        };

        for representative in representatives.values_mut() {
            if *representative == child {
                *representative = parent;
            }
        }
    }

    representatives.retain(|ty, representative| ty != representative);

    representatives
}
