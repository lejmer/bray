use std::collections::{BTreeMap, BTreeSet};

use bray_diagnostics::DiagnosticBag;
use bray_symbols::{
    CallableParameterData, CallableTypeData, GenericArgument, GenericSubstitutionData,
    GenericSubstitutionId, TraitApplicationData, TraitApplicationId, TypeData, TypeId,
};

use crate::{CheckerFactError, CheckerInfrastructureError, CheckerRequestContext, CheckerUnitView};

pub(crate) fn normalize_type_valued_members<C>(
    request: CheckerUnitView<'_, C>,
    ty: TypeId,
    diagnostics: &mut DiagnosticBag,
) -> Result<TypeId, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    TypeNormalizer {
        request,
        diagnostics,
        normalized: BTreeMap::new(),
        active: BTreeSet::new(),
    }
    .normalize_type(ty)
}

struct TypeNormalizer<'request, 'diagnostics, C>
where
    C: CheckerRequestContext + ?Sized,
{
    request: CheckerUnitView<'request, C>,
    diagnostics: &'diagnostics mut DiagnosticBag,
    normalized: BTreeMap<TypeId, TypeId>,
    active: BTreeSet<TypeId>,
}

impl<C> TypeNormalizer<'_, '_, C>
where
    C: CheckerRequestContext + ?Sized,
{
    fn normalize_type(&mut self, ty: TypeId) -> Result<TypeId, CheckerInfrastructureError> {
        if let Some(normalized) = self.normalized.get(&ty).copied() {
            return Ok(normalized);
        }

        if !self.active.insert(ty) {
            return Ok(ty);
        }

        let data = self
            .request
            .semantic_values()
            .type_data(ty)
            .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

        let normalized = match data.as_ref() {
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

                let result = self
                    .request
                    .selected_type_valued_member(subject, application, *member)
                    .map_err(checker_fact_error)?;

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
    ) -> Result<TraitApplicationId, CheckerInfrastructureError> {
        let data = self
            .request
            .semantic_values()
            .trait_application_data(application)
            .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

        let substitution = self.normalize_substitution(data.substitution())?;

        self.request
            .semantic_values()
            .intern_trait_application(TraitApplicationData::new(data.definition(), substitution))
            .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)
    }

    fn normalize_substitution(
        &mut self,
        substitution: GenericSubstitutionId,
    ) -> Result<GenericSubstitutionId, CheckerInfrastructureError> {
        let data = self
            .request
            .semantic_values()
            .generic_substitution_data(substitution)
            .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

        let parameters = data.bindings().iter().map(|binding| binding.parameter());

        let arguments = data
            .bindings()
            .iter()
            .map(|binding| match binding.argument() {
                GenericArgument::Type(ty) => self.normalize_type(ty).map(GenericArgument::Type),
                GenericArgument::Constant(term) => Ok(GenericArgument::Constant(term)),
            })
            .collect::<Result<Vec<_>, _>>()?;

        let normalized = GenericSubstitutionData::try_new(data.owner(), parameters, arguments)
            .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

        self.request
            .semantic_values()
            .intern_generic_substitution(normalized)
            .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)
    }

    fn intern(&self, data: TypeData) -> Result<TypeId, CheckerInfrastructureError> {
        self.request
            .semantic_values()
            .intern_type(data)
            .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)
    }
}

const fn checker_fact_error(error: CheckerFactError) -> CheckerInfrastructureError {
    match error {
        CheckerFactError::Cancelled => CheckerInfrastructureError::SemanticValueUnavailable,
        CheckerFactError::Infrastructure(error) => error,
    }
}
