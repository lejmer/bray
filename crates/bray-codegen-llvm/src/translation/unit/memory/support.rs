use std::sync::Arc;

use bray_codegen::CodegenFieldLayout;
use inkwell::types::BasicTypeEnum;
use inkwell::values::{BasicValueEnum, PointerValue};

pub(super) type LoadedMemoryAggregate<'context> = (
    PointerValue<'context>,
    BasicTypeEnum<'context>,
    BasicValueEnum<'context>,
    Arc<[CodegenFieldLayout]>,
);
