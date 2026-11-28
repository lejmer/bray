mod decoding;
mod encoding;
mod support;

pub use decoding::{
    ExecutableTemplateDecodeError, decode_executable_template, executable_template_references,
};
pub use support::{ExecutableTemplateReferences, ExecutableTemplateSlots};
pub(crate) use support::TemplateTable;
pub use encoding::{
    ExecutableTemplateEncodeContext, ExecutableTemplateEncodeError, encode_codegen_mir,
    encode_executable_template, encode_pre_specialized_mir,
};
