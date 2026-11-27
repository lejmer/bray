use std::sync::Arc;

use bray_codegen::CodegenTarget;
use bray_compiler_known::{
    COMPILER_KNOWN_CATALOG, RecognizedStandardLibraryDeclarationKey,
    RecognizedStandardLibraryDeclarationOwner,
};
use bray_ir::{MirHelperReference, MirStandardLibraryHelper};
use bray_symbols::{
    CallableDefinitionId, CallableInstanceData, ExactSymbolId, FunctionSymbolId,
    MemberLookupResult, PackageIdentity,
};

use super::super::super::{CodegenPreparationError, Compilation};
use super::super::specialization::ConcreteCodegenInstance;
use crate::compilation::standard_library::source_standard_library_scope_owner;
use crate::compilation::substitution::empty_substitution;
use crate::fact::CancellationToken;

impl Compilation {
    pub(super) fn concrete_standard_library_helper(
        &self,
        helper: MirStandardLibraryHelper,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
    ) -> Result<ConcreteCodegenInstance, CodegenPreparationError> {
        let callable = self.standard_library_helper_callable(helper, cancellation)?;

        self.concrete_codegen_callable(callable, [], target, cancellation)
    }

    pub(in crate::compilation) fn standard_library_helper_callable(
        &self,
        helper: MirStandardLibraryHelper,
        cancellation: &CancellationToken,
    ) -> Result<CallableInstanceData, CodegenPreparationError> {
        self.declared_standard_library_helper(helper, cancellation)?
            .ok_or_else(|| missing_helper(&MirHelperReference::StandardLibrary(helper)))
    }

    /// Returns the standard-library helper declaration, or `None` when the selected
    /// standard library does not declare it.
    pub(in crate::compilation) fn declared_standard_library_helper(
        &self,
        helper: MirStandardLibraryHelper,
        cancellation: &CancellationToken,
    ) -> Result<Option<CallableInstanceData>, CodegenPreparationError> {
        let Some(key) = RecognizedStandardLibraryDeclarationKey::try_new(standard_library_helper_key(helper)) else {
            return Ok(None);
        };

        let function = if self.package_source_authority().is_standard_library()
            && bray_standard_library::is_public_standard_library_package(self.package_identity())
        {
            self.source_standard_library_function(&key)?
        } else {
            self.imported_standard_library_function(&key, cancellation)?
        };

        let Some(definition) = function.and_then(|function| CallableDefinitionId::try_new(function.into())) else {
            return Ok(None);
        };

        let substitution = empty_substitution(self.semantic_value_store()?, definition.symbol())?;

        Ok(Some(CallableInstanceData::new(definition, substitution)))
    }

    fn source_standard_library_function(
        &self,
        key: &RecognizedStandardLibraryDeclarationKey,
    ) -> Result<Option<FunctionSymbolId>, CodegenPreparationError> {
        let Some(descriptor) = COMPILER_KNOWN_CATALOG.recognized_standard_library_declaration_by_key(key) else {
            return Ok(None);
        };

        let RecognizedStandardLibraryDeclarationOwner::Scope(scope) = descriptor.owner() else {
            return Ok(None);
        };

        let graph = self.symbol_graph()?;

        let Some(package) = graph
            .packages()
            .iter()
            .find(|package| package.identity() == self.package_identity())
        else {
            return Ok(None);
        };

        let Some(owner) = source_standard_library_scope_owner(graph, package.id(), scope) else {
            return Ok(None);
        };

        let Some(name) = descriptor.identity().name() else {
            return Ok(None);
        };

        let MemberLookupResult::Found(symbol) = graph.lookup_member(owner, name) else {
            return Ok(None);
        };

        Ok(FunctionSymbolId::try_from_any(symbol))
    }

    fn imported_standard_library_function(
        &self,
        key: &RecognizedStandardLibraryDeclarationKey,
        cancellation: &CancellationToken,
    ) -> Result<Option<FunctionSymbolId>, CodegenPreparationError> {
        let Some(package) = PackageIdentity::try_new(
            bray_standard_library::PUBLIC_STANDARD_LIBRARY_PACKAGE_IDENTITY,
        ) else {
            return Ok(None);
        };

        let imported = self.imported_symbol_skeleton_result_with_cancellation(cancellation)?;

        if imported.diagnostics().has_errors() {
            // Preserve the dependency's exact failure beyond the borrowed query result.
            return Err(CodegenPreparationError::Diagnostics(
                imported.diagnostics().clone(),
            ));
        }

        let Some(imported) = imported
            .value()
            .as_ref()
            .filter(|imported| imported.package_by_identity(&package).is_some())
        else {
            if self
                .dependency_interfaces()
                .iter()
                .any(|input| input.package() == &package)
            {
                return Ok(None);
            }

            return Err(CodegenPreparationError::MissingStandardLibrary);
        };

        let target = self.selected_target().target();

        Ok(Arc::clone(imported)
            .recognize_standard_library(&package, |rule| target.supports(rule))
            .declaration_symbol(key))
    }
}

fn missing_helper(reference: &MirHelperReference) -> CodegenPreparationError {
    panic!(
        "checked native MissingHelperInstance contract failed: {:?}",
        reference
    )
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
        MirStandardLibraryHelper::CharacterScalarValue => "StandardRuntimeCharacterScalarValue",
        MirStandardLibraryHelper::CharacterFromScalarValue => {
            "StandardRuntimeCharacterFromScalarValue"
        }
        MirStandardLibraryHelper::CharacterUtf8Length => "StandardRuntimeCharacterUtf8Length",
        MirStandardLibraryHelper::CharacterUtf8Byte => "StandardRuntimeCharacterUtf8Byte",
        MirStandardLibraryHelper::CharacterIsAlphabetic => "StandardRuntimeCharacterIsAlphabetic",
        MirStandardLibraryHelper::CharacterIsNumeric => "StandardRuntimeCharacterIsNumeric",
        MirStandardLibraryHelper::CharacterIsWhitespace => "StandardRuntimeCharacterIsWhitespace",
    }
}

#[cfg(test)]
mod tests {
    use bray_compiler_known::{COMPILER_KNOWN_CATALOG, RecognizedStandardLibraryDeclarationKey};
    use bray_ir::MirStandardLibraryHelper;
    use bray_symbols::PackageIdentity;

    use crate::fact::CancellationToken;
    use crate::test_support::source_input;
    use crate::{Compilation, CompilationRequest};

    #[test]
    fn demanded_helpers_report_an_absent_standard_library_dependency() {
        let compilation =
            crate::test_support::compilation("module app; public func value() -> u8 { return 1; }");

        for helper in MirStandardLibraryHelper::ALL {
            assert!(matches!(
                compilation.standard_library_helper_callable(helper, &CancellationToken::new()),
                Err(crate::CodegenPreparationError::MissingStandardLibrary)
            ));
        }
    }

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

using core.memory;

trusted func allocate(pos bytes: usize, pos align: usize) -> DataPointer<u8>
{
    return core.memory.null<u8>();
}

trusted func deallocate(
    pos pointer: DataPointer<u8>,
    pos bytes: usize,
    pos align: usize,
) -> unit
{
}
"#,
            0,
        );

        let request =
            CompilationRequest::new(package, vec![source]).with_standard_library_source_authority();

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
                .concrete_standard_library_helper(helper, &target, &CancellationToken::new())
                .unwrap_or_else(|error| panic!("runtime memory helper must realize: {error:?}"));
        }
    }
}
