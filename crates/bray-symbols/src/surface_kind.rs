use bray_compiler_known::CatalogDeclarationKind;
use bray_declarations::DeclarationKind;

use crate::SymbolKind;

#[derive(Clone, Copy)]
pub(crate) enum DeclarationSurfaceKind {
    Constant,
    Function,
    Predicate,
    CallableContract,
    CallableOverload,
    ImplementationOverload,
    Struct,
    Union,
    Trait,
    InherentImplementation,
    UnnamedTraitImplementation,
    NamedTraitImplementation,
    StructField,
    UnionVariant,
    UnionPayloadField,
    TypeConstructorMember,
    TypeCallableMember,
    FinalizerMember,
    DestructorMember,
    ScopeEnterMember,
    ScopeExitMember,
    TraitConstantMember,
    TraitTypeMember,
    TraitPredicateMember,
    TraitCallableMember,
    TraitFinalizerRequirement,
    TraitDestructorRequirement,
    TraitScopeEnterRequirement,
    TraitScopeExitRequirement,
    ImplementationTypeMemberBinding,
    GenericTypeParameter,
    GenericConstParameter,
    CallableParameter,
    PredicateParameter,
}

pub(crate) fn declaration_symbol_kind(
    declaration: DeclarationSurfaceKind,
    owner: SymbolKind,
) -> SymbolKind {
    let trait_implementation = matches!(
        owner,
        SymbolKind::UnnamedTraitImplementation | SymbolKind::NamedTraitImplementation
    );

    match declaration {
        DeclarationSurfaceKind::Constant if trait_implementation => {
            SymbolKind::TraitConstantFulfillment
        }
        DeclarationSurfaceKind::Constant => SymbolKind::Constant,
        DeclarationSurfaceKind::Function => SymbolKind::Function,
        DeclarationSurfaceKind::Predicate if trait_implementation => {
            SymbolKind::TraitPredicateFulfillment
        }
        DeclarationSurfaceKind::Predicate => SymbolKind::Predicate,
        DeclarationSurfaceKind::CallableContract => SymbolKind::CallableContract,
        DeclarationSurfaceKind::CallableOverload => SymbolKind::CallableOverload,
        DeclarationSurfaceKind::ImplementationOverload => SymbolKind::ImplementationOverload,
        DeclarationSurfaceKind::Struct => SymbolKind::Struct,
        DeclarationSurfaceKind::Union => SymbolKind::Union,
        DeclarationSurfaceKind::Trait => SymbolKind::Trait,
        DeclarationSurfaceKind::InherentImplementation => SymbolKind::InherentImplementation,
        DeclarationSurfaceKind::UnnamedTraitImplementation => {
            SymbolKind::UnnamedTraitImplementation
        }
        DeclarationSurfaceKind::NamedTraitImplementation => SymbolKind::NamedTraitImplementation,
        DeclarationSurfaceKind::StructField => SymbolKind::StructField,
        DeclarationSurfaceKind::UnionVariant => SymbolKind::UnionVariant,
        DeclarationSurfaceKind::UnionPayloadField => SymbolKind::UnionPayloadField,
        DeclarationSurfaceKind::TypeConstructorMember => SymbolKind::Constructor,
        DeclarationSurfaceKind::TypeCallableMember if trait_implementation => {
            SymbolKind::TraitCallableFulfillment
        }
        DeclarationSurfaceKind::TypeCallableMember => SymbolKind::TypeCallableMember,
        DeclarationSurfaceKind::FinalizerMember => SymbolKind::Finalizer,
        DeclarationSurfaceKind::DestructorMember => SymbolKind::Destructor,
        DeclarationSurfaceKind::ScopeEnterMember if trait_implementation => {
            SymbolKind::TraitScopeEnterFulfillment
        }
        DeclarationSurfaceKind::ScopeEnterMember => SymbolKind::ScopeEnter,
        DeclarationSurfaceKind::ScopeExitMember if trait_implementation => {
            SymbolKind::TraitScopeExitFulfillment
        }
        DeclarationSurfaceKind::ScopeExitMember => SymbolKind::ScopeExit,
        DeclarationSurfaceKind::TraitConstantMember => SymbolKind::TraitConstantMember,
        DeclarationSurfaceKind::TraitTypeMember => SymbolKind::TraitTypeMember,
        DeclarationSurfaceKind::TraitPredicateMember => SymbolKind::TraitPredicateMember,
        DeclarationSurfaceKind::TraitCallableMember => SymbolKind::TraitCallableMember,
        DeclarationSurfaceKind::TraitFinalizerRequirement => SymbolKind::TraitFinalizerRequirement,
        DeclarationSurfaceKind::TraitDestructorRequirement => {
            SymbolKind::TraitDestructorRequirement
        }
        DeclarationSurfaceKind::TraitScopeEnterRequirement => {
            SymbolKind::TraitScopeEnterRequirement
        }
        DeclarationSurfaceKind::TraitScopeExitRequirement => SymbolKind::TraitScopeExitRequirement,
        DeclarationSurfaceKind::ImplementationTypeMemberBinding if trait_implementation => {
            SymbolKind::TraitTypeFulfillment
        }
        DeclarationSurfaceKind::ImplementationTypeMemberBinding => SymbolKind::InherentTypeMember,
        DeclarationSurfaceKind::GenericTypeParameter => SymbolKind::GenericTypeParameter,
        DeclarationSurfaceKind::GenericConstParameter => SymbolKind::GenericConstParameter,
        DeclarationSurfaceKind::CallableParameter => SymbolKind::CallableParameter,
        DeclarationSurfaceKind::PredicateParameter => SymbolKind::PredicateParameter,
    }
}

impl From<CatalogDeclarationKind> for DeclarationSurfaceKind {
    fn from(kind: CatalogDeclarationKind) -> Self {
        match kind {
            CatalogDeclarationKind::Constant => Self::Constant,
            CatalogDeclarationKind::Function => Self::Function,
            CatalogDeclarationKind::Predicate => Self::Predicate,
            CatalogDeclarationKind::CallableContract => Self::CallableContract,
            CatalogDeclarationKind::CallableOverload => Self::CallableOverload,
            CatalogDeclarationKind::ImplementationOverload => Self::ImplementationOverload,
            CatalogDeclarationKind::Struct => Self::Struct,
            CatalogDeclarationKind::Union => Self::Union,
            CatalogDeclarationKind::Trait => Self::Trait,
            CatalogDeclarationKind::InherentImplementation => Self::InherentImplementation,
            CatalogDeclarationKind::UnnamedTraitImplementation => Self::UnnamedTraitImplementation,
            CatalogDeclarationKind::NamedTraitImplementation => Self::NamedTraitImplementation,
            CatalogDeclarationKind::StructField => Self::StructField,
            CatalogDeclarationKind::UnionVariant => Self::UnionVariant,
            CatalogDeclarationKind::UnionPayloadField => Self::UnionPayloadField,
            CatalogDeclarationKind::TypeConstructorMember => Self::TypeConstructorMember,
            CatalogDeclarationKind::TypeCallableMember => Self::TypeCallableMember,
            CatalogDeclarationKind::FinalizerMember => Self::FinalizerMember,
            CatalogDeclarationKind::DestructorMember => Self::DestructorMember,
            CatalogDeclarationKind::ScopeEnterMember => Self::ScopeEnterMember,
            CatalogDeclarationKind::ScopeExitMember => Self::ScopeExitMember,
            CatalogDeclarationKind::TraitConstantMember => Self::TraitConstantMember,
            CatalogDeclarationKind::TraitTypeMember => Self::TraitTypeMember,
            CatalogDeclarationKind::TraitPredicateMember => Self::TraitPredicateMember,
            CatalogDeclarationKind::TraitCallableMember => Self::TraitCallableMember,
            CatalogDeclarationKind::TraitFinalizerRequirement => Self::TraitFinalizerRequirement,
            CatalogDeclarationKind::TraitDestructorRequirement => Self::TraitDestructorRequirement,
            CatalogDeclarationKind::TraitScopeEnterRequirement => Self::TraitScopeEnterRequirement,
            CatalogDeclarationKind::TraitScopeExitRequirement => Self::TraitScopeExitRequirement,
            CatalogDeclarationKind::ImplementationTypeMemberBinding => {
                Self::ImplementationTypeMemberBinding
            }
        }
    }
}

impl TryFrom<DeclarationKind> for DeclarationSurfaceKind {
    type Error = ();

    fn try_from(kind: DeclarationKind) -> Result<Self, Self::Error> {
        let surface = match kind {
            DeclarationKind::Module | DeclarationKind::Using | DeclarationKind::Export => {
                return Err(());
            }
            DeclarationKind::Constant => Self::Constant,
            DeclarationKind::Function => Self::Function,
            DeclarationKind::Predicate => Self::Predicate,
            DeclarationKind::CallableContract => Self::CallableContract,
            DeclarationKind::CallableOverload => Self::CallableOverload,
            DeclarationKind::ImplementationOverload => Self::ImplementationOverload,
            DeclarationKind::Struct => Self::Struct,
            DeclarationKind::Union => Self::Union,
            DeclarationKind::Trait => Self::Trait,
            DeclarationKind::InherentImplementation => Self::InherentImplementation,
            DeclarationKind::UnnamedTraitImplementation => Self::UnnamedTraitImplementation,
            DeclarationKind::NamedTraitImplementation => Self::NamedTraitImplementation,
            DeclarationKind::StructField => Self::StructField,
            DeclarationKind::UnionVariant => Self::UnionVariant,
            DeclarationKind::UnionPayloadField => Self::UnionPayloadField,
            DeclarationKind::TypeConstructorMember => Self::TypeConstructorMember,
            DeclarationKind::TypeCallableMember => Self::TypeCallableMember,
            DeclarationKind::FinalizerMember => Self::FinalizerMember,
            DeclarationKind::DestructorMember => Self::DestructorMember,
            DeclarationKind::ScopeEnterMember => Self::ScopeEnterMember,
            DeclarationKind::ScopeExitMember => Self::ScopeExitMember,
            DeclarationKind::TraitConstantMember => Self::TraitConstantMember,
            DeclarationKind::TraitTypeMember => Self::TraitTypeMember,
            DeclarationKind::TraitPredicateMember => Self::TraitPredicateMember,
            DeclarationKind::TraitCallableMember => Self::TraitCallableMember,
            DeclarationKind::TraitFinalizerRequirement => Self::TraitFinalizerRequirement,
            DeclarationKind::TraitDestructorRequirement => Self::TraitDestructorRequirement,
            DeclarationKind::TraitScopeEnterRequirement => Self::TraitScopeEnterRequirement,
            DeclarationKind::TraitScopeExitRequirement => Self::TraitScopeExitRequirement,
            DeclarationKind::ImplementationTypeMemberBinding => {
                Self::ImplementationTypeMemberBinding
            }
            DeclarationKind::GenericTypeParameter => Self::GenericTypeParameter,
            DeclarationKind::GenericConstParameter => Self::GenericConstParameter,
            DeclarationKind::CallableParameter => Self::CallableParameter,
            DeclarationKind::PredicateParameter => Self::PredicateParameter,
        };

        Ok(surface)
    }
}
