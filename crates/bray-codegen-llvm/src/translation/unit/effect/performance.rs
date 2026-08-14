use bray_codegen::CodegenFailure;
use inkwell::IntPredicate;

use super::super::core::{PerformanceLoop, UnitTranslator};
use super::super::support::llvm;

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(super) fn begin_performance_interval(&mut self) -> Result<(), CodegenFailure> {
        let bray_codegen::RuntimeObservationMode::PerformanceInterval { inner_iterations } =
            self.request.options().runtime_observations()
        else {
            return Ok(());
        };

        self.observe_performance_interval(
            bray_runtime_abi::PERFORMANCE_INTERVAL_BEGIN_SYMBOL,
            "performance.interval.begin",
        )?;

        if inner_iterations.get() == 1 {
            return Ok(());
        }

        let entry = self
            .builder
            .get_insert_block()
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let header = self
            .types
            .context()
            .append_basic_block(self.function, "performance.iteration");

        let body = self
            .types
            .context()
            .append_basic_block(self.function, "performance.execute");

        llvm(self.builder.build_unconditional_branch(header))?;
        self.builder.position_at_end(header);

        let integer = self.types.context().i64_type();

        let iteration = llvm(
            self.builder
                .build_phi(integer, "performance.iteration.index"),
        )?;

        let status = llvm(
            self.builder
                .build_phi(integer, "performance.iteration.status"),
        )?;

        let zero = integer.const_zero();

        iteration.add_incoming(&[(&zero, entry)]);
        status.add_incoming(&[(&zero, entry)]);

        llvm(self.builder.build_unconditional_branch(body))?;
        self.builder.position_at_end(body);

        self.retain_performance_iteration()?;

        self.performance_loop = Some(PerformanceLoop {
            header,
            iteration,
            status,
            inner_iterations,
        });

        Ok(())
    }

    pub(super) fn finish_performance_interval_iteration(
        &mut self,
        status: inkwell::values::IntValue<'context>,
    ) -> Result<inkwell::values::IntValue<'context>, CodegenFailure> {
        let Some(performance_loop) = self.performance_loop.take() else {
            self.observe_performance_interval(
                bray_runtime_abi::PERFORMANCE_INTERVAL_END_SYMBOL,
                "performance.interval.end",
            )?;

            return Ok(status);
        };

        let iteration = performance_loop.iteration.as_basic_value().into_int_value();

        let aggregate_status = performance_loop.status.as_basic_value().into_int_value();

        let aggregate_status = llvm(self.builder.build_or(
            aggregate_status,
            status,
            "performance.iteration.aggregate_status",
        ))?;

        let next = llvm(self.builder.build_int_add(
            iteration,
            iteration.get_type().const_int(1, false),
            "performance.iteration.next",
        ))?;

        let more = llvm(
            self.builder.build_int_compare(
                IntPredicate::ULT,
                next,
                iteration
                    .get_type()
                    .const_int(performance_loop.inner_iterations.get(), false),
                "performance.iteration.more",
            ),
        )?;

        let body_end = self
            .builder
            .get_insert_block()
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let done = self
            .types
            .context()
            .append_basic_block(self.function, "performance.iteration.done");

        llvm(
            self.builder
                .build_conditional_branch(more, performance_loop.header, done),
        )?;

        performance_loop
            .iteration
            .add_incoming(&[(&next, body_end)]);

        performance_loop
            .status
            .add_incoming(&[(&aggregate_status, body_end)]);

        self.builder.position_at_end(done);

        self.observe_performance_interval(
            bray_runtime_abi::PERFORMANCE_INTERVAL_END_SYMBOL,
            "performance.interval.end",
        )?;

        Ok(aggregate_status)
    }

    fn retain_performance_iteration(&self) -> Result<(), CodegenFailure> {
        let side_effect = self
            .module
            .get_function("llvm.sideeffect")
            .unwrap_or_else(|| {
                self.module.add_function(
                    "llvm.sideeffect",
                    self.types.context().void_type().fn_type(&[], false),
                    None,
                )
            });

        llvm(
            self.builder
                .build_call(side_effect, &[], "performance.iteration.side_effect"),
        )?;

        Ok(())
    }

    fn observe_performance_interval(&self, symbol: &str, name: &str) -> Result<(), CodegenFailure> {
        if !matches!(
            self.request.options().runtime_observations(),
            bray_codegen::RuntimeObservationMode::PerformanceInterval { .. }
        ) {
            return Ok(());
        }

        let function = self.module.get_function(symbol).unwrap_or_else(|| {
            self.module.add_function(
                symbol,
                self.types.context().void_type().fn_type(&[], false),
                None,
            )
        });

        llvm(self.builder.build_call(function, &[], name))?;

        Ok(())
    }
}
