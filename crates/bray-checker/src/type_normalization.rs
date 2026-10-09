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
    TypeNormalizer {
        request,
        diagnostics,
        substitution: None,
        normalized: BTreeMap::new(),
        active: BTreeSet::new(),
    }
    .normalize_type(ty)
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
    let mut normalizer = TypeNormalizer {
        request,
        diagnostics,
        substitution,
        normalized: BTreeMap::new(),
        active: BTreeSet::new(),
    };

    signature.try_map_types(|ty| normalizer.normalize_type(ty))
}

struct TypeNormalizer<'request, 'diagnostics, C>
where
    C: CheckerRequestContext + ?Sized,
{
    request: &'request C,
    diagnostics: &'diagnostics mut DiagnosticBag,
    substitution: Option<GenericSubstitutionId>,
    normalized: BTreeMap<TypeId, TypeId>,
    active: BTreeSet<TypeId>,
}

impl<C> TypeNormalizer<'_, '_, C>
where
    C: CheckerRequestContext + ?Sized,
{
    fn normalize_type(
        &mut self,
        ty: TypeId,
    ) -> Result<TypeId, CheckerQueryError<C::UpstreamError>> {
        if let Some(normalized) = self.normalized.get(&ty).copied() {
            return Ok(normalized);
        }

        if !self.active.insert(ty) {
            return Ok(ty);
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
            TypeData::Error | TypeData::TypeParameter(_) | TypeData::ContextualSelf(_) => ty,
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
                let application = self.normalize_application(*application)?;

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

                self.intern(TypeData::Array {
                    element,
                    length: *length,
                })?
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
                let application = self.normalize_application(*application)?;

                self.intern(TypeData::TraitView(application))?
            }
            TypeData::OwnedIndirection { storage, target } => {
                let storage = self.normalize_type(*storage)?;
                let target = self.normalize_type(*target)?;

                self.intern(TypeData::OwnedIndirection { storage, target })?
            }
            TypeData::Callable(callable) => {
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

                // Phase behavior is immutable Arc-backed semantic data shared by the rebuilt type.
                let callable = CallableTypeData::new(
                    parameters,
                    result,
                    callable.constness(),
                    callable.trust(),
                    callable.abi(),
                    callable.dependency_contracts(),
                )
                .with_variadic(callable.is_variadic())
                .with_phase_behaviors(callable.phase_behaviors().clone());

                self.intern(TypeData::Callable(callable))?
            }
        };

        self.active.remove(&ty);
        self.normalized.insert(ty, normalized);

        Ok(normalized)
    }

    fn normalize_application(
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
                GenericArgument::Constant(term) => Ok(GenericArgument::Constant(term)),
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
