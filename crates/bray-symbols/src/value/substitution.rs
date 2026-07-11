use std::sync::Arc;

use bray_base::shared_slice;

use crate::{AnySymbolId, GenericParameterSymbolId, SymbolKind};

use super::{ConstantTermId, TypeId};

/// A validated symbol that owns an ordered generic parameter list.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct GenericOwnerId(AnySymbolId);

impl GenericOwnerId {
    /// Creates a generic owner when the symbol category can introduce generic parameters.
    pub const fn try_new(symbol: AnySymbolId) -> Option<Self> {
        if supports_generic_parameters(symbol.kind()) {
            return Some(Self(symbol));
        }

        None
    }

    /// Returns the exact symbol retained by this generic-owner adapter.
    pub const fn symbol(self) -> AnySymbolId {
        self.0
    }
}

const fn supports_generic_parameters(kind: SymbolKind) -> bool {
    matches!(
        kind,
        SymbolKind::Constant
            | SymbolKind::Function
            | SymbolKind::Predicate
            | SymbolKind::CallableContract
            | SymbolKind::Struct
            | SymbolKind::Union
            | SymbolKind::Trait
            | SymbolKind::InherentImplementation
            | SymbolKind::UnnamedTraitImplementation
            | SymbolKind::NamedTraitImplementation
            | SymbolKind::TypeCallableMember
            | SymbolKind::Constructor
            | SymbolKind::Finalizer
            | SymbolKind::Destructor
            | SymbolKind::ScopeEnter
            | SymbolKind::ScopeExit
            | SymbolKind::InherentTypeMember
            | SymbolKind::TraitCallableMember
            | SymbolKind::TraitConstantMember
            | SymbolKind::TraitTypeMember
            | SymbolKind::TraitPredicateMember
            | SymbolKind::TraitFinalizerRequirement
            | SymbolKind::TraitDestructorRequirement
            | SymbolKind::TraitScopeEnterRequirement
            | SymbolKind::TraitScopeExitRequirement
            | SymbolKind::TraitCallableFulfillment
            | SymbolKind::TraitConstantFulfillment
            | SymbolKind::TraitTypeFulfillment
            | SymbolKind::TraitPredicateFulfillment
            | SymbolKind::TraitScopeEnterFulfillment
            | SymbolKind::TraitScopeExitFulfillment
    )
}

/// Classifies one generic parameter or argument.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum GenericArgumentKind {
    /// A semantic type argument.
    Type,
    /// An open or closed constant argument.
    Constant,
}

/// One ordered argument supplied to a generic owner.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum GenericArgument {
    /// A semantic type argument.
    Type(TypeId),
    /// An open or closed constant argument.
    Constant(ConstantTermId),
}

impl GenericArgument {
    /// Returns this argument's exact category.
    pub const fn kind(self) -> GenericArgumentKind {
        match self {
            Self::Type(_) => GenericArgumentKind::Type,
            Self::Constant(_) => GenericArgumentKind::Constant,
        }
    }
}

/// One parameter-to-argument binding in source parameter order.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct GenericBinding {
    parameter: GenericParameterSymbolId,
    argument: GenericArgument,
}

impl GenericBinding {
    /// Returns the exact generic parameter being substituted.
    pub const fn parameter(self) -> GenericParameterSymbolId {
        self.parameter
    }

    /// Returns the argument supplied for the parameter.
    pub const fn argument(self) -> GenericArgument {
        self.argument
    }
}

/// Reports malformed ordered generic substitution input.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GenericSubstitutionShapeError {
    /// Parameter and argument counts differ.
    ArgumentCountMismatch {
        /// Number of declared parameters.
        parameter_count: usize,
        /// Number of supplied arguments.
        argument_count: usize,
    },
    /// One argument category differs from its parameter category.
    ArgumentKindMismatch {
        /// Stable argument ordinal.
        ordinal: u32,
        /// Parameter category.
        expected: GenericArgumentKind,
        /// Argument category.
        actual: GenericArgumentKind,
    },
    /// The parameter list exceeds the compact stable ordinal representation.
    OrdinalOverflow,
}

/// The immutable structural key for one ordered generic substitution.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct GenericSubstitutionData {
    owner: GenericOwnerId,
    bindings: Arc<[GenericBinding]>,
}

impl GenericSubstitutionData {
    /// Creates a substitution after validating arity and argument categories.
    pub fn try_new<P, A>(
        owner: GenericOwnerId,
        parameters: P,
        arguments: A,
    ) -> Result<Self, GenericSubstitutionShapeError>
    where
        P: IntoIterator<Item = GenericParameterSymbolId>,
        A: IntoIterator<Item = GenericArgument>,
    {
        let parameters: Vec<_> = parameters.into_iter().collect();
        let arguments: Vec<_> = arguments.into_iter().collect();

        let maximum_parameter_count = usize::try_from(u32::MAX).unwrap_or(usize::MAX);

        if parameters.len() > maximum_parameter_count {
            return Err(GenericSubstitutionShapeError::OrdinalOverflow);
        }

        if parameters.len() != arguments.len() {
            return Err(GenericSubstitutionShapeError::ArgumentCountMismatch {
                parameter_count: parameters.len(),
                argument_count: arguments.len(),
            });
        }

        let mut bindings = Vec::with_capacity(parameters.len());

        for (index, (parameter, argument)) in parameters.into_iter().zip(arguments).enumerate() {
            let expected = parameter_kind(parameter);
            let actual = argument.kind();

            if expected != actual {
                let ordinal = match u32::try_from(index) {
                    Ok(ordinal) => ordinal,
                    Err(_) => return Err(GenericSubstitutionShapeError::OrdinalOverflow),
                };

                return Err(GenericSubstitutionShapeError::ArgumentKindMismatch {
                    ordinal,
                    expected,
                    actual,
                });
            }

            bindings.push(GenericBinding {
                parameter,
                argument,
            });
        }

        Ok(Self {
            owner,
            bindings: shared_slice(bindings),
        })
    }

    /// Returns the exact symbol that owns this substitution.
    pub const fn owner(&self) -> GenericOwnerId {
        self.owner
    }

    /// Returns the ordered parameter-to-argument bindings.
    pub fn bindings(&self) -> &[GenericBinding] {
        &self.bindings
    }

    /// Returns the argument bound to an exact generic parameter.
    pub fn argument_for(&self, parameter: GenericParameterSymbolId) -> Option<GenericArgument> {
        self.bindings
            .iter()
            .find(|binding| binding.parameter == parameter)
            .map(|binding| binding.argument)
    }
}

const fn parameter_kind(parameter: GenericParameterSymbolId) -> GenericArgumentKind {
    match parameter {
        GenericParameterSymbolId::Type(_) => GenericArgumentKind::Type,
        GenericParameterSymbolId::Const(_) => GenericArgumentKind::Constant,
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        AnySymbolId, ConstantTermData, FunctionSymbolId, GenericArgument, GenericArgumentKind,
        GenericConstParameterSymbolId, GenericParameterSymbolId, GenericSubstitutionData,
        GenericSubstitutionShapeError, GenericTypeParameterSymbolId, SemanticValueStore,
        StructFieldSymbolId, SymbolId,
    };

    use super::{GenericOwnerId, supports_generic_parameters};
    use crate::SymbolKind;

    #[test]
    fn generic_owners_reject_non_generic_symbol_categories() {
        let symbol = SymbolId::new(3);
        let function = AnySymbolId::from(FunctionSymbolId::from_symbol_id(symbol));
        let field = AnySymbolId::from(StructFieldSymbolId::from_symbol_id(symbol));

        assert!(GenericOwnerId::try_new(function).is_some());
        assert!(GenericOwnerId::try_new(field).is_none());

        assert!(supports_generic_parameters(SymbolKind::Trait));
    }

    #[test]
    fn generic_parameter_categories_are_exact() {
        let symbol = SymbolId::new(5);

        let type_parameter =
            GenericParameterSymbolId::from(GenericTypeParameterSymbolId::from_symbol_id(symbol));

        let const_parameter =
            GenericParameterSymbolId::from(GenericConstParameterSymbolId::from_symbol_id(symbol));

        assert_ne!(type_parameter.kind(), const_parameter.kind());
    }

    #[test]
    fn substitutions_validate_arity_and_argument_categories() {
        let symbol = SymbolId::new(8);
        let function = AnySymbolId::from(FunctionSymbolId::from_symbol_id(symbol));

        let Some(owner) = GenericOwnerId::try_new(function) else {
            panic!("function must support generic substitutions");
        };

        let type_parameter = GenericTypeParameterSymbolId::from_symbol_id(SymbolId::new(9));
        let const_parameter = GenericConstParameterSymbolId::from_symbol_id(SymbolId::new(10));

        let store = match SemanticValueStore::try_new() {
            Ok(store) => store,
            Err(error) => panic!("semantic store creation failed: {error:?}"),
        };

        let term = match store.intern_constant_term(ConstantTermData::Parameter(const_parameter)) {
            Ok(term) => term,
            Err(error) => panic!("constant parameter term interning failed: {error:?}"),
        };

        assert_eq!(
            GenericSubstitutionData::try_new(
                owner,
                [GenericParameterSymbolId::from(type_parameter)],
                std::iter::empty::<GenericArgument>(),
            ),
            Err(GenericSubstitutionShapeError::ArgumentCountMismatch {
                parameter_count: 1,
                argument_count: 0,
            })
        );

        assert_eq!(
            GenericSubstitutionData::try_new(
                owner,
                [GenericParameterSymbolId::from(type_parameter)],
                [GenericArgument::Constant(term)],
            ),
            Err(GenericSubstitutionShapeError::ArgumentKindMismatch {
                ordinal: 0,
                expected: GenericArgumentKind::Type,
                actual: GenericArgumentKind::Constant,
            })
        );
    }
}
