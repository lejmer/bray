use crate::translation::unit::core::UnitTranslator;
use crate::translation::unit::support::{int_value, llvm};
use bray_codegen::CodegenFailure;
use bray_ir::MirRuntimeReference;
use bray_runtime_interface::{RuntimeAbiRole, RuntimeCapability};
use inkwell::IntPredicate;
use inkwell::values::{IntValue, StructValue};

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(super) fn initialize_host_runtime(
        &mut self,
        runtime: MirRuntimeReference,
    ) -> Result<(), CodegenFailure> {
        let returned = if runtime.role() == RuntimeAbiRole::MainThreadLaneStartup {
            let configuration = self.execution_configuration();

            self.invoke_native_runtime(runtime, &[configuration.into()])?
        } else {
            self.invoke_native_runtime(runtime, &[])?
        };

        let status = returned
            .and_then(int_value)
            .expect("runtime initialization returns its native status");

        self.require_initialization(status)
    }

    fn execution_configuration(&self) -> StructValue<'context> {
        let bray_ir::MirUnitKind::ExecutableHost(host) = self.unit.kind() else {
            panic!("runtime initialization belongs to an executable host");
        };

        let context = self.types.context();
        let usize = crate::native::pointer_integer_type(context, self.request.target());

        let pointer = context
            .ptr_type(inkwell::AddressSpace::default())
            .const_null();

        let capacity = host
            .capacity_limits()
            .tasks()
            .map_or(u64::MAX, |capacity| u64::from(capacity.get()));

        // Local and migratable placement are queue capabilities. Actual worker
        // service demand remains separate from these scalar selection bits.
        let capabilities = host
            .runtime_capabilities()
            .iter()
            .fold(7, |bits, capability| {
                bits | match capability {
                    RuntimeCapability::CooperativeExecution => 1,
                    RuntimeCapability::LocalLanes => 2,
                    RuntimeCapability::MigratableLanes => 4,
                    RuntimeCapability::BlockingLanes => 8,
                    RuntimeCapability::ComputeLanes => 16,
                    RuntimeCapability::MainThreadLane => 32,
                    RuntimeCapability::PerformanceObservation | RuntimeCapability::Reactor => 0,
                }
            });

        crate::native::runtime_configuration_type(context, self.request.target())
            .const_named_struct(&[
                usize.const_int(capacity, false).into(),
                usize.const_all_ones().into(),
                context.i32_type().const_int(capabilities, false).into(),
                pointer.into(),
                pointer.into(),
                pointer.into(),
                usize.const_zero().into(),
                usize.const_zero().into(),
            ])
    }

    fn require_initialization(&mut self, status: IntValue<'context>) -> Result<(), CodegenFailure> {
        let initialized = llvm(self.builder.build_int_compare(
            IntPredicate::EQ,
            status,
            status.get_type().const_zero(),
            "runtime.initialized",
        ))?;

        let existing = llvm(self.builder.build_int_compare(
            IntPredicate::EQ,
            status,
            status.get_type().const_int(
                u64::from(bray_runtime_abi::NativeRuntimeStatus::ALREADY_INITIALIZED.code()),
                false,
            ),
            "runtime.already_initialized",
        ))?;

        let ready = llvm(
            self.builder
                .build_or(initialized, existing, "runtime.ready"),
        )?;

        let context = self.types.context();
        let continuation = context.append_basic_block(self.function, "runtime.ready");
        let failure = context.append_basic_block(self.function, "runtime.initialization.failed");

        llvm(
            self.builder
                .build_conditional_branch(ready, continuation, failure),
        )?;

        self.builder.position_at_end(failure);

        // No task or product entry has published yet. Retire accepted execution
        // services before releasing their common host and report backing.
        for role in [
            RuntimeAbiRole::ExecutionShutdown,
            RuntimeAbiRole::StructuredShutdown,
        ] {
            let bray_ir::MirUnitKind::ExecutableHost(host) = self.unit.kind() else {
                unreachable!("runtime initialization retains its executable host");
            };

            if host.requirements().requires_role(role) {
                self.invoke_native_runtime(
                    MirRuntimeReference::new(role, self.unit.target().runtime_abi()),
                    &[],
                )?;
            }
        }

        self.translate_compiler_shutdown(context.i64_type().const_int(1, false))?;
        llvm(self.builder.build_unreachable())?;
        self.builder.position_at_end(continuation);

        Ok(())
    }
}
