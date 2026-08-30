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

impl Compilation {
    pub(super) fn codegen_runtime_source_signature(
        &self,
        role: RuntimeAbiRole,
    ) -> Result<Option<CodegenCallableSignature>, FactQueryError> {
        let signature = match role {
            RuntimeAbiRole::RuntimeInitialization => {
                let capacity = self.codegen_representation_type(RepresentationRole::ScalarUsize)?;

                let status = self.codegen_representation_type(RepresentationRole::ScalarU32)?;

                CodegenCallableSignature::new(
                    [
                        CodegenParameterMapping::direct(capacity, None, []),
                        CodegenParameterMapping::direct(capacity, None, []),
                    ],
                    CodegenResultMapping::direct(status, None, []),
                    CallableAbi::C,
                    false,
                )
            }
            RuntimeAbiRole::StructuredShutdown => {
                let status = self.codegen_representation_type(RepresentationRole::ScalarU32)?;

                CodegenCallableSignature::new(
                    [],
                    CodegenResultMapping::direct(status, None, []),
                    CallableAbi::C,
                    false,
                )
            }
            RuntimeAbiRole::ThreadAttachmentIdentity => {
                let pointer = self.codegen_opaque_pointer_type()?;
                let identity = self.codegen_representation_type(RepresentationRole::ScalarU64)?;

                CodegenCallableSignature::new(
                    [CodegenParameterMapping::direct(pointer, None, [])],
                    CodegenResultMapping::direct(identity, None, []),
                    CallableAbi::C,
                    false,
                )
            }
            RuntimeAbiRole::ThreadStaticCleanupRegistration => {
                let pointer = self.codegen_opaque_pointer_type()?;
                let status = self.codegen_representation_type(RepresentationRole::ScalarU32)?;

                CodegenCallableSignature::new(
                    [CodegenParameterMapping::direct(pointer, None, [])],
                    CodegenResultMapping::direct(status, None, []),
                    CallableAbi::C,
                    false,
                )
            }
            RuntimeAbiRole::NativeThreadExecution => {
                let pointer = self.codegen_opaque_pointer_type()?;
                let address = self.codegen_representation_type(RepresentationRole::ScalarUsize)?;
                let status = self.codegen_representation_type(RepresentationRole::ScalarU32)?;

                CodegenCallableSignature::new(
                    [
                        CodegenParameterMapping::direct(pointer, None, []),
                        CodegenParameterMapping::direct(address, None, []),
                        CodegenParameterMapping::direct(pointer, None, []),
                        CodegenParameterMapping::direct(address, None, []),
                        CodegenParameterMapping::direct(pointer, None, []),
                    ],
                    CodegenResultMapping::direct(status, None, []),
                    CallableAbi::C,
                    false,
                )
            }
            _ => return Ok(None),
        };

        Ok(Some(signature))
    }
}

pub(super) fn runtime_source_signature_matches(
    compilation: &Compilation,
    role: RuntimeAbiRole,
    signature: &CodegenCallableSignature,
    cancellation: &CancellationToken,
) -> Result<Option<bool>, FactQueryError> {
    let (parameters, result, abi) = match role {
        RuntimeAbiRole::RuntimeInitialization => (&[USIZE, USIZE][..], Some(&U32), CallableAbi::C),
        RuntimeAbiRole::SynchronousRootExecution | RuntimeAbiRole::ForeignCallbackExecution => (
            &[BYTE_POINTER, USIZE][..],
            Some(&RUN_OUTCOME),
            CallableAbi::C,
        ),
        RuntimeAbiRole::NativeThreadExecution => (
            &[BYTE_POINTER, USIZE, BYTE_POINTER, USIZE, USIZE_POINTER][..],
            Some(&U32),
            CallableAbi::C,
        ),
        RuntimeAbiRole::ThreadAttachmentIdentity => {
            (&[BYTE_POINTER][..], Some(&U64), CallableAbi::C)
        }
        RuntimeAbiRole::ThreadStaticCleanupRegistration => {
            (&[BYTE_POINTER][..], Some(&U32), CallableAbi::C)
        }
        RuntimeAbiRole::PanicReporting | RuntimeAbiRole::PanicReportDestruction => {
            (&[USIZE][..], Some(&U32), CallableAbi::C)
        }
        RuntimeAbiRole::StructuredShutdown => (&[][..], Some(&U32), CallableAbi::C),
        RuntimeAbiRole::PanicReportConstruction => (
            &[U32, U32, U32, U32, U32, U64, BYTE_POINTER, USIZE][..],
            Some(&USIZE),
            CallableAbi::C,
        ),
        _ => return Ok(None),
    };

    if signature.abi() != abi
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

    let Some(expected_result) = result else {
        return Ok(Some(matches!(
            signature.result(),
            CodegenResultMapping::Void
        )));
    };

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
