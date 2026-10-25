use bray_symbols::{
    AnySymbolId, CallableInstanceData, DefaultValueProvider, DependencyContractTemplateData,
    DependencyContractTemplateId, GenericArgument, GenericSubstitutionData, GenericSubstitutionId,
    SelfTypeContext, TypeId,
};

use crate::{CheckerQueryError, CheckerRequestContext, CheckerUnitView};

use super::super::parameters::ResultKey;

/// Exact selected body identity for a callable or a declaration-owned input provider.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum CleanupKey {
    Callable(ResultKey),
    Default {
        provider: DefaultValueProvider,
        substitution: GenericSubstitutionId,
        contextual_self: Option<(SelfTypeContext, TypeId)>,
    },
}

impl CleanupKey {
    pub(super) fn owner(self) -> AnySymbolId {
        match self {
            Self::Callable(key) => key.callable.definition().symbol(),
            Self::Default { provider, .. } => provider.symbol(),
        }
    }

    pub(super) fn substitution(self) -> GenericSubstitutionId {
        match self {
            Self::Callable(key) => key.callable.substitution(),
            Self::Default { substitution, .. } => substitution,
        }
    }

    pub(super) fn with_substitution(self, substitution: GenericSubstitutionId) -> Self {
        match self {
            Self::Callable(key) => Self::Callable(ResultKey {
                callable: CallableInstanceData::new(key.callable.definition(), substitution),
                ..key
            }),
            Self::Default {
                provider,
                contextual_self,
                ..
            } => Self::Default {
                provider,
                substitution,
                contextual_self,
            },
        }
    }

    pub(super) fn contextual_self(self) -> Option<(SelfTypeContext, TypeId)> {
        match self {
            Self::Callable(key) => key.contextual_self,
            Self::Default {
                contextual_self, ..
            } => contextual_self,
        }
    }

    pub(super) fn with_contextual_self(
        self,
        contextual_self: Option<(SelfTypeContext, TypeId)>,
    ) -> Self {
        match self {
            Self::Callable(key) => Self::Callable(ResultKey {
                contextual_self,
                ..key
            }),
            Self::Default {
                provider,
                substitution,
                ..
            } => Self::Default {
                provider,
                substitution,
                contextual_self,
            },
        }
    }

    pub(super) fn normalize<C: CheckerRequestContext + ?Sized>(
        mut self,
        request: CheckerUnitView<'_, C>,
        diagnostics: &mut bray_diagnostics::DiagnosticBag,
    ) -> Result<Self, CheckerQueryError<C::UpstreamError>> {
        let values = request.semantic_values();

        let substitution = self.substitution();
        let data = values.generic_substitution_data(substitution);

        let arguments = data
            .bindings()
            .iter()
            .map(|binding| match binding.argument() {
                GenericArgument::Type(ty) => {
                    crate::type_normalization::normalize_type_valued_members(
                        request.context(),
                        ty,
                        diagnostics,
                    )
                    .map(GenericArgument::Type)
                }
                argument @ GenericArgument::Constant(_) => Ok(argument),
            })
            .collect::<Result<Vec<_>, _>>()?;

        let substitution = GenericSubstitutionData::try_new(
            data.owner(),
            data.bindings().iter().map(|binding| binding.parameter()),
            arguments,
        )
        .expect("normalizing selected cleanup preserves generic argument kinds");

        self = self.with_substitution(
            values
                .intern_generic_substitution(substitution)
                .expect("normalized cleanup uses established semantic values"),
        );

        if let Some((context, subject)) = self.contextual_self() {
            let subject = crate::type_normalization::normalize_type_valued_members(
                request.context(),
                subject,
                diagnostics,
            )?;

            self = self.with_contextual_self(Some((context, subject)));
        }

        Ok(self)
    }

    pub(super) fn template<C: CheckerRequestContext + ?Sized>(
        self,
        request: CheckerUnitView<'_, C>,
    ) -> Result<std::sync::Arc<DependencyContractTemplateData>, CheckerQueryError<C::UpstreamError>>
    {
        let template = match self {
            Self::Callable(key) => {
                request.callable_result_dependencies(key.callable.definition().callable_symbol())?
            }
            Self::Default { provider, .. } => request.default_result_dependencies(provider)?,
        };

        self.instantiate(request, template)
    }

    pub(super) fn instantiate<C: CheckerRequestContext + ?Sized>(
        self,
        request: CheckerUnitView<'_, C>,
        template: DependencyContractTemplateId,
    ) -> Result<std::sync::Arc<DependencyContractTemplateData>, CheckerQueryError<C::UpstreamError>>
    {
        match self {
            Self::Callable(key) => {
                super::super::witness::instantiate_template(request, template, key)
            }
            Self::Default {
                substitution,
                contextual_self,
                ..
            } => {
                let values = request.semantic_values();

                let template = values
                    .substitute_dependency_contract(template, substitution)
                    .expect("selected default cleanup retains its enclosing substitution");

                let template = match contextual_self {
                    Some((context, subject)) => values
                        .substitute_contextual_self_in_dependency_contract(
                            template, context, subject,
                        )
                        .expect("selected default cleanup retains its lexical Self context"),
                    None => template,
                };

                Ok(values.dependency_contract_template_data(template))
            }
        }
    }
}
