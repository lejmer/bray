use super::core::UnitTranslator;
use super::support::{llvm, nonzero_integer};
use bray_codegen::CodegenFailure;
use bray_ir::MirTerminatorKind;
use inkwell::values::BasicValueEnum;

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(super) fn translate_frame_dispatch(&mut self) -> Result<(), CodegenFailure> {
        let (Some(frame_context), Some(dispatch)) = (self.frame_context, self.frame_dispatch)
        else {
            return Ok(());
        };

        let descriptor = self
            .unit
            .frame_descriptor()
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let context = self.frame_context_argument()?;

        self.builder.position_at_end(dispatch);

        let pointer = llvm(
            self.builder.build_int_to_ptr(
                context,
                self.types
                    .context()
                    .ptr_type(inkwell::AddressSpace::default()),
                "frame.context",
            ),
        )?;

        let state_pointer =
            llvm(
                self.builder
                    .build_struct_gep(frame_context, pointer, 0, "frame.state.pointer"),
            )?;

        let state = llvm(self.builder.build_load(
            self.types.context().i32_type(),
            state_pointer,
            "frame.state",
        ))?
        .into_int_value();

        let cancellation_pointer = llvm(self.builder.build_struct_gep(
            frame_context,
            pointer,
            2,
            "frame.cancellation.pointer",
        ))?;

        let cancellation = llvm(self.builder.build_load(
            self.types.context().i8_type(),
            cancellation_pointer,
            "frame.cancellation",
        ))?
        .into_int_value();

        let mut cases = Vec::with_capacity(descriptor.states().len());

        let fallback = self
            .types
            .context()
            .append_basic_block(self.function, "frame.invalid-state");

        for state in descriptor.states() {
            let entry = self.block(state.entry())?;

            let cancellation_entry =
                self.unit
                    .blocks()
                    .iter()
                    .find_map(|block| match block.terminator().kind() {
                        MirTerminatorKind::Suspend {
                            resume_state,
                            cancellation,
                            ..
                        } if *resume_state == state.state() => {
                            cancellation.as_ref().map(bray_ir::MirCleanupEdge::edge)
                        }
                        _ => None,
                    });

            if cancellation_entry.is_some_and(|entry| !entry.arguments().is_empty()) {
                return Err(CodegenFailure::GeneratedModuleInvariant);
            }

            let cancellation_entry = if state.state().raw() == 0 {
                descriptor.inactive_cleanup()
            } else {
                cancellation_entry.map(bray_ir::MirEdge::target)
            };

            let entry = if state.state().raw() == 0 {
                let initial_dispatch = self
                    .types
                    .context()
                    .append_basic_block(self.function, "frame.initial-entry");

                let entry_code = |selected: bray_ir::MirFrameEntry| {
                    cancellation
                        .get_type()
                        .const_int(u64::from(selected.code()), false)
                };

                let mut entries = vec![(entry_code(bray_ir::MirFrameEntry::Body), entry)];

                if let Some(cleanup) = cancellation_entry {
                    entries.push((
                        entry_code(bray_ir::MirFrameEntry::CaptureCleanup),
                        self.block(cleanup)?,
                    ));
                }

                if let Some((quiescence, destruction)) = descriptor.capture_abandonment() {
                    entries.push((
                        entry_code(bray_ir::MirFrameEntry::CaptureQuiescence),
                        self.block(quiescence)?,
                    ));

                    entries.push((
                        entry_code(bray_ir::MirFrameEntry::CaptureDestruction),
                        self.block(destruction)?,
                    ));
                }

                self.builder.position_at_end(initial_dispatch);
                llvm(self.builder.build_switch(cancellation, fallback, &entries))?;

                initial_dispatch
            } else if let Some(cancellation_entry) = cancellation_entry {
                let state_dispatch = self
                    .types
                    .context()
                    .append_basic_block(self.function, "frame.state.dispatch");

                self.builder.position_at_end(state_dispatch);

                let requested =
                    nonzero_integer(&self.builder, cancellation, "frame.cancellation.requested")?;

                llvm(self.builder.build_conditional_branch(
                    requested,
                    self.block(cancellation_entry)?,
                    entry,
                ))?;

                state_dispatch
            } else {
                entry
            };

            cases.push((
                self.types
                    .context()
                    .i32_type()
                    .const_int(u64::from(state.state().raw()), false),
                entry,
            ));
        }

        self.builder.position_at_end(dispatch);

        llvm(self.builder.build_switch(state, fallback, &cases))?;

        self.builder.position_at_end(fallback);

        let failure =
            crate::native::frame_progress_type(self.types.context()).const_named_struct(&[
                self.types.context().i32_type().const_int(4, false).into(),
                self.types.context().i32_type().const_zero().into(),
                self.types.context().i64_type().const_zero().into(),
            ]);

        self.return_frame_progress(failure.into())?;

        Ok(())
    }

    pub(super) fn return_frame_progress(
        &self,
        progress: BasicValueEnum<'context>,
    ) -> Result<(), CodegenFailure> {
        crate::native::return_frame_result(
            self.types.context(),
            &self.builder,
            self.function,
            self.request.target(),
            bray_runtime_interface::ProtectedFrameOperation::Resume,
            progress,
        )
    }

    pub(super) fn frame_context_argument(
        &self,
    ) -> Result<inkwell::values::IntValue<'context>, CodegenFailure> {
        self.function
            .get_nth_param(crate::native::frame_parameter_index(
                self.request.target(),
                bray_runtime_interface::ProtectedFrameOperation::Resume,
                0,
            ))
            .and_then(super::support::int_value)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)
    }
}
