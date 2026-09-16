use super::core::UnitTranslator;
use bray_codegen::CodegenFailure;
use bray_ir::{MirCall, MirCallArgument};
use inkwell::values::BasicValueEnum;
use std::collections::BTreeMap;

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(super) fn evaluate_call_arguments(
        &mut self,
        call: &MirCall,
    ) -> Result<Vec<BasicValueEnum<'context>>, CodegenFailure> {
        let mut receiver = None;
        let mut parameters = BTreeMap::new();

        for argument in call.arguments() {
            match argument {
                MirCallArgument::Receiver { value, .. } => {
                    let value = self.operand(value)?;

                    if receiver.replace(value).is_some() {
                        panic!("checked MIR translation violated an established compiler contract");
                    }
                }
                MirCallArgument::Explicit { ordinal, value, .. } => {
                    let value = self.operand(value)?;

                    if parameters.insert(*ordinal, value).is_some() {
                        panic!("checked MIR translation violated an established compiler contract");
                    }
                }
            }
        }

        let mut arguments = Vec::with_capacity(call.arguments().len());
        arguments.extend(receiver);

        for (expected, (ordinal, value)) in (0_u32..).zip(parameters) {
            if ordinal != expected {
                panic!("checked MIR translation violated an established compiler contract");
            }

            arguments.push(value);
        }

        Ok(arguments)
    }
}
