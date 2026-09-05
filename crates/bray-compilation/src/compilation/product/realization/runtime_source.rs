use bray_codegen::{CodegenCallableSignature, CodegenParameterMapping, CodegenResultMapping};
use bray_compiler_known::RepresentationRole;
use bray_runtime_interface::RuntimeAbiRole;
use bray_symbols::CallableAbi;

use super::super::super::Compilation;
use super::super::super::foreign::{AbiField, abi_type_matches};
use crate::fact::{CancellationToken, FactQueryError};

const BYTE_POINTER: AbiField = AbiField::Pointer(RepresentationRole::ScalarU8);
const USIZE_POINTER: AbiField = AbiField::Pointer(RepresentationRole::ScalarUsize);
const U32: AbiField = AbiField::Scalar(RepresentationRole::ScalarU32);
const U64: AbiField = AbiField::Scalar(RepresentationRole::ScalarU64);
const USIZE: AbiField = AbiField::Scalar(RepresentationRole::ScalarUsize);
const RUN_OUTCOME_FIELDS: &[AbiField] = &[U32, USIZE];
const RUN_OUTCOME: AbiField = AbiField::Struct(RUN_OUTCOME_FIELDS);
const PRODUCT_OBSERVATION_FIELDS: &[AbiField] = &[
    U32, U32, USIZE, USIZE, USIZE, USIZE, USIZE, USIZE, U64, U64, U64, U64,
];
const PRODUCT_OBSERVATION: AbiField = AbiField::Struct(PRODUCT_OBSERVATION_FIELDS);

macro_rules! define_runtime_source_fields {
    ($( $role:ident {
        $documentation:literal, $name:literal,
        native: ($($symbol:ident = $native:literal, [$($native_parameter:ident),*] -> $native_result:ident)?),
        call_hook: ($($hook:ident)?),
        compiler: $abi:ident [$($parameter:ident),*] -> $result:ident,
        owner: $owner:ident, availability: $availability:ident,
        bootstrap: ($($bootstrap:literal)?), host_control: $host_control:literal,
        capabilities: [$($capability:ident),*],
        effects: [$($effect:ident),*]
    })+) => {
        fn runtime_source_fields(role: RuntimeAbiRole) -> Option<(&'static [AbiField], AbiField)> {
            match role {
                $(RuntimeAbiRole::$role => define_runtime_source_fields!(
                    @bootstrap ($($bootstrap)?) ($([$($native_parameter),*] -> $native_result)?)
                ),)+
            }
        }
    };
    (@bootstrap () $native:tt) => { None };
    (@bootstrap ($name:literal) ([$($parameter:ident),*] -> $result:ident)) => {
        Some((&[$(define_runtime_source_fields!(@field $parameter),)*], define_runtime_source_fields!(@field $result)))
    };
    (@field U32) => { U32 };
    (@field U64) => { U64 };
    (@field Usize) => { USIZE };
    (@field Pointer) => { BYTE_POINTER };
    (@field PointerUsize) => { USIZE_POINTER };
    (@field ProductObservation) => { PRODUCT_OBSERVATION };
    (@field RunOutcome) => { RUN_OUTCOME };
}

bray_runtime_abi::runtime_role_catalog!(define_runtime_source_fields);

pub(super) fn runtime_source_signature_matches(
    compilation: &Compilation,
    role: RuntimeAbiRole,
    signature: &CodegenCallableSignature,
    cancellation: &CancellationToken,
) -> Result<Option<bool>, FactQueryError> {
    let Some((parameters, result)) = runtime_source_fields(role) else {
        return Ok(None);
    };

    if signature.abi() != CallableAbi::C
        || signature.is_variadic()
        || signature.has_panic_report_context()
        || signature.parameters().len() != parameters.len()
    {
        return Ok(Some(false));
    }

    for (parameter, expected) in signature.parameters().iter().zip(parameters) {
        let CodegenParameterMapping::Direct {
            ty,
            extension: None,
            attributes,
        } = parameter
        else {
            return Ok(Some(false));
        };

        if !attributes.is_empty() || !abi_type_matches(compilation, *ty, expected, cancellation)? {
            return Ok(Some(false));
        }
    }

    let expected_result = &result;

    let CodegenResultMapping::Direct {
        ty,
        extension: None,
        attributes,
    } = signature.result()
    else {
        return Ok(Some(false));
    };

    Ok(Some(
        attributes.is_empty() && abi_type_matches(compilation, *ty, expected_result, cancellation)?,
    ))
}

#[cfg(test)]
mod tests {
    use bray_codegen::{CodegenCallableSignature, CodegenParameterMapping, CodegenResultMapping};
    use bray_compiler_known::RepresentationRole;
    use bray_runtime_interface::RuntimeAbiRole;
    use bray_symbols::{CallableAbi, NamedTypeSymbolId, SymbolOrigin};

    use super::runtime_source_signature_matches;
    use crate::CancellationToken;
    use crate::compilation::substitution::named_type;
    use crate::test_support::compilation;

    #[test]
    fn callback_boundary_roles_accept_the_structural_c_run_outcome() {
        let compilation = compilation(concat!(
            "module app;\n",
            "@copy\n",
            "@layout(c)\n",
            "struct RunOutcome\n",
            "{\n",
            "    state: u32;\n",
            "    payload: usize;\n",
            "}\n",
        ));

        let symbols = compilation
            .symbol_graph()
            .unwrap_or_else(|error| panic!("symbol graph must be available: {error:?}"));

        let outcome = symbols
            .structures()
            .iter()
            .find(|symbol| symbol.origin() == SymbolOrigin::Source)
            .unwrap_or_else(|| panic!("fixture must declare RunOutcome"));

        let values = compilation
            .semantic_value_store()
            .unwrap_or_else(|error| panic!("semantic values must be available: {error:?}"));

        let outcome = named_type(values, NamedTypeSymbolId::Struct(outcome.id()))
            .unwrap_or_else(|error| panic!("RunOutcome type must be available: {error:?}"));

        let pointer = compilation
            .codegen_opaque_pointer_type()
            .unwrap_or_else(|error| panic!("opaque pointer must be available: {error:?}"));

        let usize = compilation
            .codegen_representation_type(RepresentationRole::ScalarUsize)
            .unwrap_or_else(|error| panic!("usize must be available: {error:?}"));

        let signature = CodegenCallableSignature::new(
            [
                CodegenParameterMapping::direct(pointer, None, []),
                CodegenParameterMapping::direct(usize, None, []),
            ],
            CodegenResultMapping::direct(outcome, None, []),
            CallableAbi::C,
            false,
        );

        assert_eq!(
            runtime_source_signature_matches(
                &compilation,
                RuntimeAbiRole::SynchronousRootExecution,
                &signature,
                &CancellationToken::new(),
            )
            .unwrap_or_else(|error| panic!("runtime ABI must validate: {error:?}")),
            Some(true)
        );
    }
}
