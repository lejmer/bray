use std::collections::{BTreeMap, BTreeSet};

use bray_compiler_known::{
    CatalogDeclarationSignature, CatalogGenericParameterKind, CompilerKnownCatalog,
    CompilerKnownDeclarationId,
};

use crate::allocator::SymbolIdAllocator;
use crate::build::{declaration_symbol_id, receiver_owner};
use crate::record::{DeclarationSymbolIdentity, ReceiverParameterSymbol};
use crate::{
    AnySymbolId, CompilerKnownSymbolBuildError, ReceiverParameterSymbolId, SymbolKey, SymbolKind,
    SymbolName, SymbolOrdinal, SymbolOrigin, SynthesizedSymbolKey,
};

use super::support::required_declaration_value;

pub(super) struct CompilerKnownSignatureSymbols {
    pub(super) declarations: Vec<(
        CompilerKnownDeclarationId,
        AnySymbolId,
        DeclarationSymbolIdentity,
    )>,
    pub(super) receivers: Vec<ReceiverParameterSymbol>,
    pub(super) completion_children: BTreeMap<CompilerKnownDeclarationId, Box<[AnySymbolId]>>,
}

pub(super) fn allocate_signature_symbols(
    catalog: &CompilerKnownCatalog,
    declaration_symbols: &BTreeMap<CompilerKnownDeclarationId, AnySymbolId>,
    declaration_keys: &BTreeMap<CompilerKnownDeclarationId, SymbolKey>,
    allocator: &mut SymbolIdAllocator,
) -> Result<CompilerKnownSignatureSymbols, CompilerKnownSymbolBuildError> {
    let mut declarations = Vec::new();
    let mut receivers = Vec::new();
    let mut completion_children = BTreeMap::new();
    let declared_names = declared_names(catalog, declaration_symbols)?;

    for descriptor in catalog.compiler_known_declarations() {
        let owner = required_declaration_value(declaration_symbols, descriptor.id()).copied()?;

        if owner.kind() == SymbolKind::TrustedCapability {
            continue;
        }

        let owner_key = required_declaration_value(declaration_keys, descriptor.id())?;

        let Some(surface) = catalog.declaration_surface(descriptor.surface()) else {
            return Err(CompilerKnownSymbolBuildError::InvalidDeclarationSurface {
                declaration: descriptor.id(),
            });
        };

        let signature = surface.signature();
        let children = signature_children(&signature, &declared_names);
        let origin = declaration_origin(descriptor.implementation_hook().is_some());

        let mut generic_children = Vec::new();
        let mut callable_children = Vec::new();
        let mut predicate_children = Vec::new();

        for child in children {
            let SignatureChild {
                kind,
                ordinal,
                name,
            } = child;

            let raw_id = allocator.next()?;

            let Some(symbol) = declaration_symbol_id(raw_id, kind.symbol_kind()) else {
                return Err(CompilerKnownSymbolBuildError::InvalidDeclarationSurface {
                    declaration: descriptor.id(),
                });
            };

            // Each child key owns the shared immutable identity of its declaration owner.
            let key = SymbolKey::synthesized(kind.key(owner_key.clone(), ordinal));

            let identity = match name {
                Some(name) => DeclarationSymbolIdentity::compiler_known_parameter(
                    key,
                    owner,
                    descriptor.id(),
                    descriptor.surface(),
                    origin,
                    name,
                ),
                None => DeclarationSymbolIdentity::compiler_known(
                    key,
                    owner,
                    descriptor.id(),
                    descriptor.surface(),
                    origin,
                ),
            };

            match kind {
                SignatureChildKind::GenericType
                | SignatureChildKind::GenericConst
                | SignatureChildKind::InferredType
                | SignatureChildKind::InferredConst => generic_children.push(symbol),
                SignatureChildKind::Callable => callable_children.push(symbol),
                SignatureChildKind::Predicate => predicate_children.push(symbol),
            }

            declarations.push((descriptor.id(), symbol, identity));
        }

        let mut receiver = None;

        if let Some(callable) = receiver_owner(owner, signature.is_static()) {
            let id = ReceiverParameterSymbolId::from_symbol_id(allocator.next()?);

            // The receiver key independently owns the same immutable declaration identity.
            let key =
                SymbolKey::synthesized(SynthesizedSymbolKey::receiver_parameter(owner_key.clone()));

            receivers.push(ReceiverParameterSymbol::new(id, key, callable));

            receiver = Some(AnySymbolId::from(id));
        }

        generic_children.extend(receiver);
        generic_children.extend(callable_children);
        generic_children.extend(predicate_children);

        if !generic_children.is_empty() {
            completion_children.insert(descriptor.id(), generic_children.into_boxed_slice());
        }
    }

    Ok(CompilerKnownSignatureSymbols {
        declarations,
        receivers,
        completion_children,
    })
}

pub(super) fn order_completion_children(
    mut completion_children: BTreeMap<AnySymbolId, Box<[AnySymbolId]>>,
    signature_children: BTreeMap<AnySymbolId, Box<[AnySymbolId]>>,
) -> BTreeMap<AnySymbolId, Box<[AnySymbolId]>> {
    for (owner, signature_children) in signature_children {
        let nested_children = match completion_children.remove(&owner) {
            Some(children) => children,
            None => Box::new([]),
        };

        let mut ordered = Vec::with_capacity(nested_children.len());

        ordered.extend(signature_children.iter().copied());

        ordered.extend(
            nested_children
                .iter()
                .copied()
                .filter(|child| !signature_children.contains(child)),
        );

        completion_children.insert(owner, ordered.into_boxed_slice());
    }

    completion_children
}

#[derive(Clone)]
struct SignatureChild {
    kind: SignatureChildKind,
    ordinal: SymbolOrdinal,
    name: Option<SymbolName>,
}

#[derive(Clone, Copy)]
enum SignatureChildKind {
    GenericType,
    GenericConst,
    Callable,
    Predicate,
    InferredType,
    InferredConst,
}

impl SignatureChildKind {
    const fn symbol_kind(self) -> SymbolKind {
        match self {
            Self::GenericType => SymbolKind::GenericTypeParameter,
            Self::GenericConst => SymbolKind::GenericConstParameter,
            Self::Callable => SymbolKind::CallableParameter,
            Self::Predicate => SymbolKind::PredicateParameter,
            Self::InferredType => SymbolKind::GenericTypeParameter,
            Self::InferredConst => SymbolKind::GenericConstParameter,
        }
    }

    fn key(self, owner: SymbolKey, ordinal: SymbolOrdinal) -> SynthesizedSymbolKey {
        match self {
            Self::GenericType => {
                SynthesizedSymbolKey::declared_generic_type_parameter(owner, ordinal)
            }
            Self::GenericConst => {
                SynthesizedSymbolKey::declared_generic_const_parameter(owner, ordinal)
            }
            Self::Callable => SynthesizedSymbolKey::callable_parameter(owner, ordinal),
            Self::Predicate => SynthesizedSymbolKey::predicate_parameter(owner, ordinal),
            Self::InferredType => {
                SynthesizedSymbolKey::inferred_implementation_type_parameter(owner, ordinal)
            }
            Self::InferredConst => {
                SynthesizedSymbolKey::inferred_implementation_const_parameter(owner, ordinal)
            }
        }
    }
}

fn signature_children(
    signature: &CatalogDeclarationSignature,
    declared_names: &BTreeSet<SymbolName>,
) -> Vec<SignatureChild> {
    let mut children = Vec::new();
    let mut generic_ordinal = 0_u32;

    for parameter in signature.generic_parameters() {
        let kind = match parameter.kind() {
            CatalogGenericParameterKind::Type => SignatureChildKind::GenericType,
            CatalogGenericParameterKind::Const => SignatureChildKind::GenericConst,
        };

        children.push(SignatureChild {
            kind,
            ordinal: SymbolOrdinal::new(generic_ordinal),
            name: None,
        });

        generic_ordinal = generic_ordinal.saturating_add(1);
    }

    children.extend(
        signature.callable_parameters().iter().enumerate().map(|(ordinal, name)| SignatureChild {
            kind: SignatureChildKind::Callable,
            ordinal: SymbolOrdinal::new(ordinal as u32),
            name: SymbolName::try_new(name.clone()),
        }),
    );

    children.extend(
        signature.predicate_parameters().iter().enumerate().map(|(ordinal, name)| SignatureChild {
            kind: SignatureChildKind::Predicate,
            ordinal: SymbolOrdinal::new(ordinal as u32),
            name: SymbolName::try_new(name.clone()),
        }),
    );

    for candidate in signature.implementation_parameter_candidates() {
        let Some(name) = SymbolName::try_new(candidate.name()) else {
            continue;
        };

        if declared_names.contains(&name) {
            continue;
        }

        let kind = match candidate.kind() {
            CatalogGenericParameterKind::Type => SignatureChildKind::InferredType,
            CatalogGenericParameterKind::Const => SignatureChildKind::InferredConst,
        };

        children.push(SignatureChild {
            kind,
            ordinal: SymbolOrdinal::new(generic_ordinal),
            name: Some(name),
        });

        generic_ordinal = generic_ordinal.saturating_add(1);
    }

    children
}

fn declared_names(
    catalog: &CompilerKnownCatalog,
    declaration_symbols: &BTreeMap<CompilerKnownDeclarationId, AnySymbolId>,
) -> Result<BTreeSet<SymbolName>, CompilerKnownSymbolBuildError> {
    let mut names = BTreeSet::new();

    for descriptor in catalog.compiler_known_declarations() {
        let symbol = required_declaration_value(declaration_symbols, descriptor.id()).copied()?;

        let Some(surface) = catalog.declaration_surface(descriptor.surface()) else {
            return Err(CompilerKnownSymbolBuildError::InvalidDeclarationSurface {
                declaration: descriptor.id(),
            });
        };

        if let Some(name) = super::lookup::member_name(surface, symbol.kind()) {
            names.insert(name);
        }
    }

    Ok(names)
}

const fn declaration_origin(has_implementation_hook: bool) -> SymbolOrigin {
    if has_implementation_hook {
        SymbolOrigin::CompilerProvided
    } else {
        SymbolOrigin::CompilerKnown
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use bray_compiler_known::COMPILER_KNOWN_CATALOG;

    use super::signature_children;
    use crate::compiler_known::test_support::declaration_key;
    use crate::{
        CompilerKnownSymbolProvider, FunctionSymbolId, NamedTraitImplementationSymbolId,
        PredicateSymbolId, StructSymbolId, SymbolKind, SymbolProvider,
    };

    #[test]
    fn generated_signature_children_follow_direct_declaration_lists_only() {
        let provider = match CompilerKnownSymbolProvider::build() {
            Ok(provider) => provider,
            Err(error) => panic!("generated compiler-known symbols should build: {error:?}"),
        };

        let Some(pointer) =
            provider.declaration_symbol::<StructSymbolId>(&declaration_key("RawPointer"))
        else {
            panic!("RawPointer should have a struct symbol");
        };

        let Some(copy) =
            provider.declaration_symbol::<FunctionSymbolId>(&declaration_key("MemoryCopy"))
        else {
            panic!("MemoryCopy should have a function symbol");
        };

        let Some(valid_read) =
            provider.declaration_symbol::<PredicateSymbolId>(&declaration_key("ValidRead"))
        else {
            panic!("ValidRead should have a predicate symbol");
        };

        let Some(heap_storage) = provider.declaration_symbol::<NamedTraitImplementationSymbolId>(
            &declaration_key("HeapStorageImplementation"),
        ) else {
            panic!("HeapStorageImplementation should have an implementation symbol");
        };

        let Some(pointer) = provider.symbol(pointer) else {
            panic!("RawPointer symbol should resolve");
        };

        let Some(copy) = provider.symbol(copy) else {
            panic!("MemoryCopy symbol should resolve");
        };

        let Some(valid_read) = provider.symbol(valid_read) else {
            panic!("ValidRead symbol should resolve");
        };

        let Some(heap_storage) = provider.symbol(heap_storage) else {
            panic!("HeapStorageImplementation symbol should resolve");
        };

        assert_eq!(pointer.generic_type_parameters().len(), 1);
        assert_eq!(copy.parameters().len(), 3);
        assert_eq!(valid_read.generic_type_parameters().len(), 1);
        assert_eq!(valid_read.parameters().len(), 2);
        assert_eq!(heap_storage.generic_type_parameters().len(), 1);

        assert_eq!(
            provider
                .completion_children(pointer.id().into())
                .iter()
                .map(|child| child.kind())
                .collect::<Vec<_>>(),
            [SymbolKind::GenericTypeParameter, SymbolKind::StructField]
        );

        let unary = COMPILER_KNOWN_CATALOG
            .compiler_known_declarations()
            .iter()
            .find(|descriptor| descriptor.key().as_str() == "UnaryCallable");

        let Some(unary) = unary else {
            panic!("UnaryCallable descriptor should exist");
        };

        let Some(surface) = COMPILER_KNOWN_CATALOG.declaration_surface(unary.surface()) else {
            panic!("UnaryCallable surface should exist");
        };

        assert_eq!(
            signature_children(&surface.signature(), &BTreeSet::new()).len(),
            1
        );
    }
}
