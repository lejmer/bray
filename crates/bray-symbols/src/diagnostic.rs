use bray_diagnostics::{
    DiagnosticCallableAbi, DiagnosticCallableExecution, DiagnosticInterfaceDeclarationIdentity,
    DiagnosticInterfaceSymbolIdentity, DiagnosticInterfaceSymbolKind,
    DiagnosticInterfaceSynthesizedIdentity,
};

use crate::{
    CallableAbi, CallableExecution, ExternalDeclarationIdentity, ExternalSymbolKey,
    ExternalSymbolKeyData, ModulePathKey, SymbolKey, SymbolKeyData, SymbolKind, SymbolOrdinal,
    SymbolRootKey, SynthesizedSymbolRole,
};

/// Converts a callable ABI into its locale-neutral diagnostic category.
pub const fn diagnostic_callable_abi(abi: CallableAbi) -> DiagnosticCallableAbi {
    match abi {
        CallableAbi::Bray => DiagnosticCallableAbi::Bray,
        CallableAbi::C => DiagnosticCallableAbi::C,
        CallableAbi::System => DiagnosticCallableAbi::System,
    }
}

/// Converts a callable execution mode into its locale-neutral diagnostic category.
pub const fn diagnostic_callable_execution(
    execution: CallableExecution,
) -> DiagnosticCallableExecution {
    match execution {
        CallableExecution::Synchronous => DiagnosticCallableExecution::Synchronous,
        CallableExecution::Asynchronous => DiagnosticCallableExecution::Asynchronous,
    }
}

/// Converts a stable compiler symbol key into its locale-neutral diagnostic identity.
pub fn diagnostic_symbol_identity(key: &SymbolKey) -> DiagnosticInterfaceSymbolIdentity {
    match key.data() {
        SymbolKeyData::Root(root) => diagnostic_symbol_root_identity(root),
        SymbolKeyData::Module { owner, path } => DiagnosticInterfaceSymbolIdentity::Module {
            owner: Box::new(diagnostic_symbol_root_identity(owner)),
            path: diagnostic_module_path(path),
        },
        SymbolKeyData::CompilerKnownDeclaration { key, kind } => {
            DiagnosticInterfaceSymbolIdentity::CompilerKnownDeclaration {
                key: key.as_str().to_owned(),
                kind: diagnostic_symbol_kind(*kind),
            }
        }
        SymbolKeyData::SourceDeclaration {
            owner,
            kind,
            declaration,
        } => DiagnosticInterfaceSymbolIdentity::SourceDeclaration {
            owner: Box::new(diagnostic_symbol_identity(owner)),
            kind: diagnostic_symbol_kind(*kind),
            declaration: declaration.raw(),
        },
        SymbolKeyData::Synthesized(synthesized) => DiagnosticInterfaceSymbolIdentity::Synthesized {
            owner: Box::new(diagnostic_symbol_identity(synthesized.subject())),
            identity: diagnostic_synthesized_identity(synthesized.role(), synthesized.ordinal()),
        },
        SymbolKeyData::External(external) => diagnostic_external_symbol_identity(external),
    }
}

/// Converts a semantic symbol category into its closed diagnostic category.
pub const fn diagnostic_symbol_kind(kind: SymbolKind) -> DiagnosticInterfaceSymbolKind {
    use DiagnosticInterfaceSymbolKind as Diagnostic;
    use SymbolKind as Symbol;

    match kind {
        Symbol::CompilerKnownEnvironment => Diagnostic::CompilerKnownEnvironment,
        Symbol::Package => Diagnostic::Package,
        Symbol::Module => Diagnostic::Module,
        Symbol::TrustedCapability => Diagnostic::TrustedCapability,
        Symbol::Constant => Diagnostic::Constant,
        Symbol::Function => Diagnostic::Function,
        Symbol::Predicate => Diagnostic::Predicate,
        Symbol::CallableContract => Diagnostic::CallableContract,
        Symbol::CallableOverload => Diagnostic::CallableOverload,
        Symbol::ImplementationOverload => Diagnostic::ImplementationOverload,
        Symbol::Struct => Diagnostic::Struct,
        Symbol::Union => Diagnostic::Union,
        Symbol::Trait => Diagnostic::Trait,
        Symbol::InherentImplementation => Diagnostic::InherentImplementation,
        Symbol::UnnamedTraitImplementation => Diagnostic::UnnamedTraitImplementation,
        Symbol::NamedTraitImplementation => Diagnostic::NamedTraitImplementation,
        Symbol::StructField => Diagnostic::StructField,
        Symbol::UnionVariant => Diagnostic::UnionVariant,
        Symbol::UnionPayloadField => Diagnostic::UnionPayloadField,
        Symbol::TypeCallableMember => Diagnostic::TypeCallableMember,
        Symbol::Constructor => Diagnostic::Constructor,
        Symbol::Finalizer => Diagnostic::Finalizer,
        Symbol::Destructor => Diagnostic::Destructor,
        Symbol::ScopeEnter => Diagnostic::ScopeEnter,
        Symbol::ScopeExit => Diagnostic::ScopeExit,
        Symbol::InherentTypeMember => Diagnostic::InherentTypeMember,
        Symbol::TraitCallableMember => Diagnostic::TraitCallableMember,
        Symbol::TraitConstantMember => Diagnostic::TraitConstantMember,
        Symbol::TraitTypeMember => Diagnostic::TraitTypeMember,
        Symbol::TraitPredicateMember => Diagnostic::TraitPredicateMember,
        Symbol::TraitFinalizerRequirement => Diagnostic::TraitFinalizerRequirement,
        Symbol::TraitDestructorRequirement => Diagnostic::TraitDestructorRequirement,
        Symbol::TraitScopeEnterRequirement => Diagnostic::TraitScopeEnterRequirement,
        Symbol::TraitScopeExitRequirement => Diagnostic::TraitScopeExitRequirement,
        Symbol::TraitCallableFulfillment => Diagnostic::TraitCallableFulfillment,
        Symbol::TraitConstantFulfillment => Diagnostic::TraitConstantFulfillment,
        Symbol::TraitTypeFulfillment => Diagnostic::TraitTypeFulfillment,
        Symbol::TraitPredicateFulfillment => Diagnostic::TraitPredicateFulfillment,
        Symbol::TraitScopeEnterFulfillment => Diagnostic::TraitScopeEnterFulfillment,
        Symbol::TraitScopeExitFulfillment => Diagnostic::TraitScopeExitFulfillment,
        Symbol::GenericTypeParameter => Diagnostic::GenericTypeParameter,
        Symbol::GenericConstParameter => Diagnostic::GenericConstParameter,
        Symbol::CallableParameter => Diagnostic::CallableParameter,
        Symbol::PredicateParameter => Diagnostic::PredicateParameter,
        Symbol::ReceiverParameter => Diagnostic::ReceiverParameter,
        Symbol::CallableParameterDefaultProvider => Diagnostic::CallableParameterDefaultProvider,
        Symbol::StructFieldDefaultProvider => Diagnostic::StructFieldDefaultProvider,
        Symbol::UnionPayloadDefaultProvider => Diagnostic::UnionPayloadDefaultProvider,
        Symbol::LocalBinding => Diagnostic::LocalBinding,
        Symbol::LocalConstant => Diagnostic::LocalConstant,
        Symbol::AnonymousCallable => Diagnostic::AnonymousCallable,
        Symbol::AnonymousCallableParameter => Diagnostic::AnonymousCallableParameter,
        Symbol::PostconditionResult => Diagnostic::PostconditionResult,
    }
}

fn diagnostic_external_symbol_identity(
    key: &ExternalSymbolKey,
) -> DiagnosticInterfaceSymbolIdentity {
    match key.data() {
        ExternalSymbolKeyData::Package(package) => {
            DiagnosticInterfaceSymbolIdentity::Package(package.as_str().to_owned())
        }
        ExternalSymbolKeyData::Module { package, path } => {
            DiagnosticInterfaceSymbolIdentity::Module {
                owner: Box::new(diagnostic_external_symbol_identity(package)),
                path: diagnostic_module_path(path),
            }
        }
        ExternalSymbolKeyData::Declaration {
            owner,
            kind,
            identity,
        } => DiagnosticInterfaceSymbolIdentity::Declaration {
            owner: Box::new(diagnostic_external_symbol_identity(owner)),
            kind: diagnostic_symbol_kind(*kind),
            identity: match identity {
                ExternalDeclarationIdentity::Name(name) => {
                    DiagnosticInterfaceDeclarationIdentity::Name(name.as_str().to_owned())
                }
                ExternalDeclarationIdentity::Ordinal(ordinal) => {
                    DiagnosticInterfaceDeclarationIdentity::Ordinal(ordinal.raw())
                }
            },
        },
        ExternalSymbolKeyData::Synthesized {
            owner,
            role,
            ordinal,
        } => DiagnosticInterfaceSymbolIdentity::Synthesized {
            owner: Box::new(diagnostic_external_symbol_identity(owner)),
            identity: diagnostic_synthesized_identity(*role, *ordinal),
        },
    }
}

fn diagnostic_symbol_root_identity(root: &SymbolRootKey) -> DiagnosticInterfaceSymbolIdentity {
    match root {
        SymbolRootKey::CompilerKnownEnvironment => {
            DiagnosticInterfaceSymbolIdentity::CompilerKnownEnvironment
        }
        SymbolRootKey::Package(package) => {
            DiagnosticInterfaceSymbolIdentity::Package(package.as_str().to_owned())
        }
    }
}

fn diagnostic_module_path(path: &ModulePathKey) -> Box<[String]> {
    path.segments()
        .map(str::to_owned)
        .collect::<Vec<_>>()
        .into_boxed_slice()
}

fn diagnostic_synthesized_identity(
    role: SynthesizedSymbolRole,
    ordinal: Option<SymbolOrdinal>,
) -> DiagnosticInterfaceSynthesizedIdentity {
    use DiagnosticInterfaceSynthesizedIdentity as Diagnostic;
    use SynthesizedSymbolRole as Role;

    match (role, ordinal) {
        (Role::ReceiverParameter, None) => Diagnostic::ReceiverParameter,
        (Role::DeclaredGenericTypeParameter, Some(ordinal)) => {
            Diagnostic::DeclaredGenericTypeParameter(ordinal.raw())
        }
        (Role::DeclaredGenericConstParameter, Some(ordinal)) => {
            Diagnostic::DeclaredGenericConstParameter(ordinal.raw())
        }
        (Role::CallableParameter, Some(ordinal)) => Diagnostic::CallableParameter(ordinal.raw()),
        (Role::PredicateParameter, Some(ordinal)) => Diagnostic::PredicateParameter(ordinal.raw()),
        (Role::InferredImplementationTypeParameter, Some(ordinal)) => {
            Diagnostic::InferredImplementationTypeParameter(ordinal.raw())
        }
        (Role::InferredImplementationConstParameter, Some(ordinal)) => {
            Diagnostic::InferredImplementationConstParameter(ordinal.raw())
        }
        (Role::CallableParameterDefaultProvider, None) => {
            Diagnostic::CallableParameterDefaultProvider
        }
        (Role::StructFieldDefaultProvider, None) => Diagnostic::StructFieldDefaultProvider,
        (Role::UnionPayloadDefaultProvider, None) => Diagnostic::UnionPayloadDefaultProvider,
        _ => unreachable!("symbol keys enforce synthesized-role ordinal shape"),
    }
}
