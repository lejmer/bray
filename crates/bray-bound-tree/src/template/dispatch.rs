use bray_symbols::{GenericSubstitutionId, SymbolKey, TypeId};

/// The exact checked witness source selected for a callable operation.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CheckedTemplateDispatch<
    Declaration = SymbolKey,
    Substitution = GenericSubstitutionId,
    Implementation = SymbolKey,
    Type = TypeId,
> {
    /// A concrete implementation and its checked generic application.
    Implementation(Implementation, Substitution),
    /// A surrounding declaration constraint in declaration order.
    Constraint {
        /// The generic declaration supplying the witness.
        owner: Declaration,
        /// The selected constraint's ordinal.
        ordinal: bray_symbols::SymbolOrdinal,
    },
    /// The witness realizing the selected trait member's default body.
    TraitDefault {
        /// The reached subject requiring the default witness.
        subject: Type,
        /// The trait-owned substitution of the exact requirement.
        substitution: Substitution,
    },
}

impl<D, S, I, T> CheckedTemplateDispatch<D, S, I, T> {
    pub(super) fn try_map_references<O, P, Q, Y, E>(
        &self,
        ty: &mut impl FnMut(&T) -> Result<Y, E>,
        declaration: &mut impl FnMut(&D) -> Result<O, E>,
        substitution: &mut impl FnMut(&S) -> Result<P, E>,
        implementation: &mut impl FnMut(&I) -> Result<Q, E>,
    ) -> Result<CheckedTemplateDispatch<O, P, Q, Y>, E> {
        Ok(match self {
            Self::Implementation(witness, application) => CheckedTemplateDispatch::Implementation(
                implementation(witness)?,
                substitution(application)?,
            ),
            Self::Constraint { owner, ordinal } => CheckedTemplateDispatch::Constraint {
                owner: declaration(owner)?,
                ordinal: *ordinal,
            },
            Self::TraitDefault {
                subject,
                substitution: application,
            } => CheckedTemplateDispatch::TraitDefault {
                subject: ty(subject)?,
                substitution: substitution(application)?,
            },
        })
    }
}

impl CheckedTemplateDispatch {
    /// Resolves the retained trait route without selecting or rechecking an implementation.
    pub fn trait_dispatch<E: From<bray_symbols::SemanticValueStoreError>>(
        &self,
        values: &bray_symbols::SemanticValueStore,
        symbol: impl FnOnce(&SymbolKey) -> Result<bray_symbols::AnySymbolId, E>,
    ) -> Result<Option<bray_symbols::TraitConstraintDispatch>, E> {
        Ok(match self {
            Self::Implementation(..) => None,
            Self::Constraint { owner, ordinal } => {
                Some(bray_symbols::TraitConstraintDispatch::Constraint {
                    owner: bray_symbols::GenericOwnerId::try_new(symbol(owner)?)
                        .expect("checked callable constraint retains its generic owner"),
                    ordinal: *ordinal,
                })
            }
            Self::TraitDefault {
                subject,
                substitution,
            } => {
                let owner = values.generic_substitution_data(*substitution).owner();

                let bray_symbols::AnySymbolId::Trait(definition) = owner.symbol() else {
                    panic!("checked default dispatch retains its trait");
                };

                let application = values.intern_trait_application(
                    bray_symbols::TraitApplicationData::new(definition, *substitution),
                )?;

                Some(bray_symbols::TraitConstraintDispatch::TraitDefault(
                    bray_symbols::ImplementationRequirementKey::new(*subject, application),
                ))
            }
        })
    }
}
