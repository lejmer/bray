use std::sync::Arc;

use bray_compiler_known::{
    COMPILER_KNOWN_CATALOG, RecognizedStandardLibraryDeclarationKey,
    RecognizedStandardLibraryDeclarationOwner,
};
use bray_symbols::{
    CallableDefinitionId, CallableInstanceData, ExactSymbolId, FunctionSymbolId,
    MemberLookupResult, PackageIdentity,
};
use bray_codegen::CodegenTarget;
use bray_ir::{MirHelperReference, MirStandardLibraryHelper};

use super::super::super::{CodegenPreparationError, Compilation};
use super::super::specialization::ConcreteCodegenInstance;
use crate::compilation::substitution::empty_substitution;
use crate::compilation::standard_library::source_standard_library_scope_owner;
use crate::fact::CancellationToken;

impl Compilation {
    pub(super) fn concrete_standard_library_helper(
        &self,
        helper: MirStandardLibraryHelper,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
    ) -> Result<ConcreteCodegenInstance, CodegenPreparationError> {
        let reference = MirHelperReference::StandardLibrary(helper);
        let key = standard_library_helper_key(helper);

        let key = RecognizedStandardLibraryDeclarationKey::try_new(key)
            .ok_or_else(|| missing_helper(&reference))?;

        let callable =
            self.recognized_standard_library_callable(&key, &reference, cancellation)?;

        self.concrete_codegen_callable(callable, [], target, cancellation)
    }

    fn recognized_standard_library_callable(
        &self,
        key: &RecognizedStandardLibraryDeclarationKey,
        reference: &MirHelperReference,
        cancellation: &CancellationToken,
    ) -> Result<CallableInstanceData, CodegenPreparationError> {
        let function = if self.package_source_authority().is_standard_library()
            && bray_standard_library::is_public_standard_library_package(self.package_identity())
        {
            self.source_standard_library_function(key, reference)?
        } else {
            self.imported_standard_library_function(key, reference, cancellation)?
        };

        let definition = CallableDefinitionId::try_new(function.into())
            .ok_or_else(|| missing_helper(reference))?;

        let substitution =
            empty_substitution(self.semantic_value_store()?, definition.symbol())?;

        Ok(CallableInstanceData::new(definition, substitution))
    }

    fn source_standard_library_function(
        &self,
        key: &RecognizedStandardLibraryDeclarationKey,
        reference: &MirHelperReference,
    ) -> Result<FunctionSymbolId, CodegenPreparationError> {
        let descriptor = COMPILER_KNOWN_CATALOG
            .recognized_standard_library_declaration_by_key(key)
            .ok_or_else(|| missing_helper(reference))?;

        let RecognizedStandardLibraryDeclarationOwner::Scope(scope) = descriptor.owner() else {
            return Err(missing_helper(reference));
        };

        let graph = self.symbol_graph()?;

        let package = graph
            .packages()
            .iter()
            .find(|package| package.identity() == self.package_identity())
            .ok_or_else(|| missing_helper(reference))?;

        let owner = source_standard_library_scope_owner(&graph, package.id(), scope)
            .ok_or_else(|| missing_helper(reference))?;

        let Some(name) = descriptor.identity().name() else {
            return Err(missing_helper(reference));
        };

        let MemberLookupResult::Found(symbol) = graph.lookup_member(owner, name) else {
            return Err(missing_helper(reference));
        };

        FunctionSymbolId::try_from_any(symbol)
            .ok_or_else(|| missing_helper(reference))
    }

    fn imported_standard_library_function(
        &self,
        key: &RecognizedStandardLibraryDeclarationKey,
        reference: &MirHelperReference,
        cancellation: &CancellationToken,
    ) -> Result<FunctionSymbolId, CodegenPreparationError> {
        let package = PackageIdentity::try_new(
            bray_standard_library::PUBLIC_STANDARD_LIBRARY_PACKAGE_IDENTITY,
        )
        .ok_or_else(|| missing_helper(reference))?;

        let imported = self.imported_symbol_skeleton_result_with_cancellation(cancellation)?;

        let imported = imported
            .value()
            .as_ref()
            .ok_or_else(|| missing_helper(reference))?;

        let target = self.selected_target().target();

        Arc::clone(imported)
            .recognize_standard_library(&package, |rule| target.supports(rule))
            .declaration_symbol(key)
            .ok_or_else(|| missing_helper(reference))
    }
}

fn missing_helper(reference: &MirHelperReference) -> CodegenPreparationError {
    CodegenPreparationError::MissingHelperInstance(reference.clone())
}

fn standard_library_helper_key(helper: MirStandardLibraryHelper) -> &'static str {
    match helper {
        MirStandardLibraryHelper::MemoryAllocate => "StandardRuntimeMemoryAllocate",
        MirStandardLibraryHelper::MemoryDeallocate => "StandardRuntimeMemoryDeallocate",
        MirStandardLibraryHelper::StringScalarCount => "StandardRuntimeStringScalarCount",
        MirStandardLibraryHelper::StringEquals => "StandardRuntimeStringEquals",
        MirStandardLibraryHelper::StringScalarAt => "StandardRuntimeStringScalarAt",
        MirStandardLibraryHelper::StringScalarSlice => "StandardRuntimeStringScalarSlice",
        MirStandardLibraryHelper::StringFromUtf8 => "StandardRuntimeStringFromUtf8",
        MirStandardLibraryHelper::CharacterScalarValue => {
            "StandardRuntimeCharacterScalarValue"
        }
        MirStandardLibraryHelper::CharacterFromScalarValue => {
            "StandardRuntimeCharacterFromScalarValue"
        }
        MirStandardLibraryHelper::CharacterUtf8Length => {
            "StandardRuntimeCharacterUtf8Length"
        }
        MirStandardLibraryHelper::CharacterUtf8Byte => "StandardRuntimeCharacterUtf8Byte",
        MirStandardLibraryHelper::CharacterIsAlphabetic => {
            "StandardRuntimeCharacterIsAlphabetic"
        }
        MirStandardLibraryHelper::CharacterIsNumeric => {
            "StandardRuntimeCharacterIsNumeric"
        }
        MirStandardLibraryHelper::CharacterIsWhitespace => {
            "StandardRuntimeCharacterIsWhitespace"
        }
    }
}

#[cfg(test)]
mod tests {
    use bray_compiler_known::{
        COMPILER_KNOWN_CATALOG, RecognizedStandardLibraryDeclarationKey,
    };
    use bray_ir::MirStandardLibraryHelper;
    use bray_symbols::PackageIdentity;

    use crate::fact::CancellationToken;
    use crate::test_support::source_input;
    use crate::{Compilation, CompilationRequest};

    #[test]
    fn every_mir_standard_library_helper_has_a_catalog_declaration() {
        for helper in MirStandardLibraryHelper::ALL {
            let key = RecognizedStandardLibraryDeclarationKey::try_new(
                super::standard_library_helper_key(helper),
            )
            .unwrap_or_else(|| panic!("standard-library helper key must be valid"));

            assert!(
                COMPILER_KNOWN_CATALOG
                    .recognized_standard_library_declaration_by_key(&key)
                    .is_some(),
                "{helper:?} has no catalog declaration",
            );
        }
    }

    #[test]
    fn source_runtime_memory_helpers_are_recognized() {
        let package = PackageIdentity::try_new(
            bray_standard_library::PUBLIC_STANDARD_LIBRARY_PACKAGE_IDENTITY,
        )
        .unwrap_or_else(|| panic!("standard-library package identity must be valid"));

        let source = source_input(
            r#"trusted internal module std.runtime.memory;

trusted func allocate(pos bytes: usize, pos align: usize) -> RawPointer<u8>
{
    return core.memory.null<u8>();
}

trusted func deallocate(
    pos pointer: RawPointer<u8>,
    pos bytes: usize,
    pos align: usize,
) -> unit
{
}
"#,
            0,
        );

        let request = CompilationRequest::new(package, vec![source])
            .with_standard_library_source_authority();

        let compilation = Compilation::load(request)
            .unwrap_or_else(|error| panic!("test compilation must load: {error:?}"));

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:#?}",
            compilation.check_diagnostics()
        );

        let target = compilation
            .selected_target()
            .target()
            .codegen_target()
            .unwrap_or_else(|error| panic!("code generation target must resolve: {error:?}"));

        for helper in [
            MirStandardLibraryHelper::MemoryAllocate,
            MirStandardLibraryHelper::MemoryDeallocate,
        ] {
            compilation
                .concrete_standard_library_helper(
                    helper,
                    &target,
                    &CancellationToken::new(),
                )
                .unwrap_or_else(|error| {
                    panic!("runtime memory helper must realize: {error:?}")
                });
        }
    }
}
