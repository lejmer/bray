use std::collections::BTreeMap;

use bray_compiler_known::{
    CatalogDeclarationSignature, CatalogGenericParameterKind, CompilerKnownCatalog,
    CompilerKnownDeclarationId,
};

use crate::allocator::SymbolIdAllocator;
use crate::build::{declaration_symbol_id, receiver_owner};
use crate::record::{DeclarationSymbolIdentity, ReceiverParameterSymbol};
use crate::{
    AnySymbolId, CompilerKnownSymbolBuildError, ReceiverParameterSymbolId, SymbolKey, SymbolKind,
    SymbolOrdinal, SymbolOrigin, SynthesizedSymbolKey,
};

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

    for descriptor in catalog.compiler_known_declarations() {
        let Some(owner) = declaration_symbols.get(&descriptor.id()).copied() else {
            return Err(CompilerKnownSymbolBuildError::InvalidDeclarationSurface {
                declaration: descriptor.id(),
            });
        };

        let Some(owner_key) = declaration_keys.get(&descriptor.id()) else {
            return Err(CompilerKnownSymbolBuildError::InvalidDeclarationSurface {
                declaration: descriptor.id(),
            });
        };

        let Some(surface) = catalog.declaration_surface(descriptor.surface()) else {
            return Err(CompilerKnownSymbolBuildError::InvalidDeclarationSurface {
                declaration: descriptor.id(),
            });
        };

        let signature = surface.signature();
        let children = signature_children(&signature);
        let origin = declaration_origin(descriptor.implementation_hook().is_some());

        let mut generic_children = Vec::new();
        let mut callable_children = Vec::new();

        for child in children {
            let raw_id = allocator.next()?;
            let Some(symbol) = declaration_symbol_id(raw_id, child.kind.symbol_kind()) else {
                return Err(CompilerKnownSymbolBuildError::InvalidDeclarationSurface {
                    declaration: descriptor.id(),
                });
            };

            // Each child key owns the shared immutable identity of its declaration owner.
            let key = SymbolKey::synthesized(child.kind.key(owner_key.clone(), child.ordinal));
            let identity = DeclarationSymbolIdentity::compiler_known(
                key,
                owner,
                descriptor.id(),
                descriptor.surface(),
                origin,
            );

            match child.kind {
                SignatureChildKind::GenericType | SignatureChildKind::GenericConst => {
                    generic_children.push(symbol);
                }
                SignatureChildKind::Callable => callable_children.push(symbol),
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

#[derive(Clone, Copy)]
struct SignatureChild {
    kind: SignatureChildKind,
    ordinal: SymbolOrdinal,
}

#[derive(Clone, Copy)]
enum SignatureChildKind {
    GenericType,
    GenericConst,
    Callable,
}

impl SignatureChildKind {
    const fn symbol_kind(self) -> SymbolKind {
        match self {
            Self::GenericType => SymbolKind::GenericTypeParameter,
            Self::GenericConst => SymbolKind::GenericConstParameter,
            Self::Callable => SymbolKind::CallableParameter,
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
        }
    }
}

fn signature_children(signature: &CatalogDeclarationSignature) -> Vec<SignatureChild> {
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
        });

        generic_ordinal = generic_ordinal.saturating_add(1);
    }

    children.extend(
        (0..signature.callable_parameters()).map(|ordinal| SignatureChild {
            kind: SignatureChildKind::Callable,
            ordinal: SymbolOrdinal::new(ordinal),
        }),
    );

    children
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
    use bray_compiler_known::COMPILER_KNOWN_CATALOG;

    use super::signature_children;
    use crate::compiler_known::test_support::declaration_key;
    use crate::{
        CompilerKnownSymbolProvider, FunctionSymbolId, StructSymbolId, SymbolKind, SymbolProvider,
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

        let Some(pointer) = provider.symbol(pointer) else {
            panic!("RawPointer symbol should resolve");
        };

        let Some(copy) = provider.symbol(copy) else {
            panic!("MemoryCopy symbol should resolve");
        };

        assert_eq!(pointer.generic_type_parameters().len(), 1);
        assert_eq!(copy.parameters().len(), 3);

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

        assert_eq!(signature_children(&surface.signature()).len(), 1);
    }
}
