use bray_compiler_known::{COMPILER_KNOWN_CATALOG, RecognizedStandardLibraryScopeId};
use bray_symbols::{AnySymbolId, ModuleOwnerId, ModulePathKey, PackageSymbolId, SymbolGraph};

use bray_binder::BindingQueryContext;
use bray_bound_tree::{
    BoundReferenceTarget, ConstructionInputId, ConstructionTarget, SelectedConstruction,
    SelectedConstructionInput,
};
use bray_checker::{ExecutionCondition, TrustedCallContract};

use super::Compilation;
use super::binder::{CompilationBindingContext, binding_query_error};
use crate::fact::{CancellationToken, FactQueryError};

impl Compilation {
    pub(super) fn trusted_storage_formation(
        &self,
        construction: &SelectedConstruction,
        cancellation: &CancellationToken,
    ) -> Result<Option<TrustedCallContract>, FactQueryError> {
        let ConstructionTarget::Struct(structure) = construction.target() else {
            return Ok(None);
        };

        let context = self.binding_context(cancellation)?;

        let allocation =
            recognized_declaration(&context, structure.into(), "StandardRawAllocation")?;

        let buffer = recognized_declaration(&context, structure.into(), "StandardRawBuffer")?;

        if !allocation && !buffer {
            return Ok(None);
        }

        let mut contract = TrustedCallContract {
            completes: true,
            result_is_witness: true,
            ..Default::default()
        };

        let mut fields = std::collections::BTreeMap::new();

        for input in construction.inputs() {
            if let SelectedConstructionInput::Explicit {
                input: ConstructionInputId::StructField(field),
                expression,
                ..
            } = input
            {
                let reference = BoundReferenceTarget::Surface((*field).into());

                contract.arguments.insert(reference, (*expression).into());

                if let Some(name) = context
                    .member_name((*field).into())
                    .map_err(binding_query_error)?
                {
                    if name.as_str() == "pointer" {
                        contract.transferred_inputs.push(reference);
                    }

                    fields.insert(name.as_str(), ExecutionCondition::Input(reference.into()));
                }
            }
        }

        let Some(pointer) = fields.get("pointer") else {
            contract.requirements.push(ExecutionCondition::Unknown);

            return Ok(Some(contract));
        };

        let values = self.semantic_value_store()?;
        let data = values.type_data(construction.result_type());

        let bray_symbols::TypeData::Named { substitution, .. } = data.as_ref() else {
            unreachable!("selected structure construction retains its named result type");
        };

        let arguments = values
            .generic_substitution_data(*substitution)
            .bindings()
            .iter()
            .map(|binding| binding.argument())
            .collect::<Vec<_>>();

        let predicate = |key: &str,
                         generic: &[bray_symbols::GenericArgument],
                         arguments: Vec<ExecutionCondition>|
         -> Result<_, FactQueryError> {
            let key = bray_compiler_known::CompilerKnownDeclarationKey::try_new(key)
                .expect("closed memory predicate key is valid");

            let symbol = context
                .symbols()
                .compiler_known_provider()
                .declaration_symbol::<bray_symbols::PredicateSymbolId>(&key)
                .expect("available raw storage owner requires its core predicate declarations");

            let substitution = storage_contract_substitution(&context, symbol.into(), generic)?;

            Ok(ExecutionCondition::Trusted(std::sync::Arc::new(
                ExecutionCondition::Predicate(symbol.into(), substitution, arguments.into()),
            )))
        };

        if allocation {
            let (Some(bytes), Some(align)) = (fields.get("bytes"), fields.get("align")) else {
                contract.requirements.push(ExecutionCondition::Unknown);

                return Ok(Some(contract));
            };

            contract.requirements.push(predicate(
                "OwnedAllocation",
                &[],
                vec![pointer.clone(), bytes.clone(), align.clone()],
            )?);
        } else {
            let (Some(capacity), Some(initialized)) =
                (fields.get("capacity"), fields.get("initialized"))
            else {
                contract.requirements.push(ExecutionCondition::Unknown);

                return Ok(Some(contract));
            };

            let owner = context
                .containing_symbol(structure.into())
                .map_err(binding_query_error)?
                .expect("selected storage owner belongs to its recognized module");

            let layout_query = |name: &str| -> Result<_, FactQueryError> {
                let bray_symbols::MemberLookupResult::Found(symbol) = context
                    .lookup_member(owner, name)
                    .map_err(binding_query_error)?
                else {
                    return Ok(ExecutionCondition::Unknown);
                };

                let definition = bray_symbols::CallableDefinitionId::try_new(symbol)
                    .expect("standard memory layout query is callable");

                let substitution = storage_contract_substitution(&context, symbol, &arguments)?;

                Ok(ExecutionCondition::Call(
                    bray_symbols::CallableInstanceData::new(definition, substitution),
                    std::sync::Arc::new([]),
                ))
            };

            let bytes = ExecutionCondition::Operation(
                bray_bound_tree::BoundOperator::Multiply,
                vec![layout_query("stride_of")?, capacity.clone()].into(),
            );

            contract.requirements.extend([
                predicate(
                    "OwnedAllocation",
                    &[],
                    vec![pointer.clone(), bytes, layout_query("align_of")?],
                )?,
                predicate(
                    "ValidWrite",
                    &arguments,
                    vec![pointer.clone(), capacity.clone()],
                )?,
                predicate("AlignedFor", &arguments, vec![pointer.clone()])?,
                predicate(
                    "InitializedRangeAs",
                    &arguments,
                    vec![pointer.clone(), initialized.clone()],
                )?,
                ExecutionCondition::Operation(
                    bray_bound_tree::BoundOperator::LessEqual,
                    vec![initialized.clone(), capacity.clone()].into(),
                ),
            ]);
        }

        for condition in &contract.requirements {
            if matches!(condition, ExecutionCondition::Trusted(_)) {
                contract.guarantees.push(condition.clone());
            } else {
                contract.postconditions.push(condition.clone());
            }
        }

        Ok(Some(contract))
    }
}

fn storage_contract_substitution(
    context: &CompilationBindingContext<'_>,
    symbol: AnySymbolId,
    arguments: &[bray_symbols::GenericArgument],
) -> Result<bray_symbols::GenericSubstitutionId, FactQueryError> {
    let parameters = if context.symbols().symbol_key(symbol).is_some() {
        super::binder::generic_parameter_ids(context.symbols(), symbol)
    } else {
        super::binder::generic_parameter_ids(
            context
                .imported_symbols()
                .map_err(binding_query_error)?
                .expect("selected imported storage contract retains its provider"),
            symbol,
        )
    }
    .map_err(binding_query_error)?;

    let owner = bray_symbols::GenericOwnerId::try_new(symbol)
        .expect("storage contract declaration owns its generic parameters");

    let substitution = bray_symbols::GenericSubstitutionData::try_new(
        owner,
        parameters,
        arguments.iter().copied(),
    )
    .expect("closed storage contract has its declared generic arity");

    context
        .semantic_values()
        .intern_generic_substitution(substitution)
        .map_err(FactQueryError::SemanticValueStore)
}

pub(super) fn recognized_declaration(
    context: &CompilationBindingContext<'_>,
    symbol: AnySymbolId,
    key: &str,
) -> Result<bool, FactQueryError> {
    let key = bray_compiler_known::RecognizedStandardLibraryDeclarationKey::try_new(key)
        .expect("closed storage descriptor key is valid");

    let descriptor = COMPILER_KNOWN_CATALOG
        .recognized_standard_library_declaration_by_key(&key)
        .expect("closed storage descriptor is present");

    let bray_compiler_known::RecognizedStandardLibraryDeclarationOwner::Scope(scope) =
        descriptor.owner()
    else {
        unreachable!("raw storage owner belongs directly to its standard memory scope");
    };

    let path = COMPILER_KNOWN_CATALOG
        .recognized_standard_library_scope(scope)
        .expect("recognized owner has a scope")
        .path();

    let Some(symbol_key) = context.symbol_key(symbol).map_err(binding_query_error)? else {
        return Ok(false);
    };

    Ok(match symbol_key.data() {
        bray_symbols::SymbolKeyData::SourceDeclaration { owner, .. } => {
            context
                .compilation()
                .package_source_authority()
                .is_standard_library()
                && context.member_name(symbol).map_err(binding_query_error)?
                    .map(bray_symbols::SymbolName::as_str) == descriptor.identity().name()
                && matches!(owner.data(), bray_symbols::SymbolKeyData::Module { owner: bray_symbols::SymbolRootKey::Package(package), path: module } if package.as_str() == bray_standard_library::PUBLIC_STANDARD_LIBRARY_PACKAGE_IDENTITY && module.segments().eq(path.segments()))
        }
        bray_symbols::SymbolKeyData::External(key) => {
            if key.package_identity().as_str()
                != bray_standard_library::PUBLIC_STANDARD_LIBRARY_PACKAGE_IDENTITY {
                return Ok(false);
            }

            let imported = context.compilation()
                .imported_symbol_skeleton_result_with_cancellation(context.cancellation())?;
            let Some(imported) = imported.value() else {
                return Ok(false);
            };
            let target = context.compilation().selected_target().target();

            std::sync::Arc::clone(imported)
                .recognize_standard_library(key.package_identity(), |rule| target.supports(rule))
                .descriptor(symbol) == Some(descriptor.id())
        }
        _ => false,
    })
}

pub(super) fn source_standard_library_scope_owner(
    symbols: &SymbolGraph,
    package: PackageSymbolId,
    scope: RecognizedStandardLibraryScopeId,
) -> Option<AnySymbolId> {
    let scope = COMPILER_KNOWN_CATALOG.recognized_standard_library_scope(scope)?;
    let path = ModulePathKey::try_new(scope.path().segments())?;
    let module = symbols.module_by_path(ModuleOwnerId::from(package), &path)?;

    Some(module.id().into())
}
