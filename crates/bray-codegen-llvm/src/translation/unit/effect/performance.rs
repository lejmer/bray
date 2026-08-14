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
            retain_performance_value(self.types.context(), &self.builder, status)?;

            self.observe_performance_interval(
                bray_runtime_abi::PERFORMANCE_INTERVAL_END_SYMBOL,
                "performance.interval.end",
            )?;

            return Ok(status);
        };

        retain_performance_value(self.types.context(), &self.builder, status)?;

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

fn retain_performance_value<'context>(
    context: &'context inkwell::context::Context,
    builder: &inkwell::builder::Builder<'context>,
    status: inkwell::values::IntValue<'context>,
) -> Result<(), CodegenFailure> {
    let function_type = context
        .void_type()
        .fn_type(&[status.get_type().into()], false);

    let barrier = context.create_inline_asm(
        function_type,
        String::new(),
        "r,~{memory}".to_owned(),
        true,
        false,
        None,
        false,
    );

    llvm(builder.build_indirect_call(
        function_type,
        barrier,
        &[status.into()],
        "performance.iteration.value_barrier",
    ))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use inkwell::context::Context;

    use super::retain_performance_value;

    #[test]
    fn performance_barrier_consumes_each_root_status() {
        let context = Context::create();
        let module = context.create_module("performance_barrier");
        let builder = context.create_builder();
        let integer = context.i64_type();
        let function_type = context.void_type().fn_type(&[integer.into()], false);
        let function = module.add_function("root", function_type, None);
        let entry = context.append_basic_block(function, "entry");

        builder.position_at_end(entry);

        let status = function
            .get_first_param()
            .unwrap_or_else(|| panic!("barrier fixture must have a status parameter"))
            .into_int_value();

        retain_performance_value(&context, &builder, status)
            .unwrap_or_else(|error| panic!("performance barrier must generate: {error:?}"));

        builder
            .build_return(None)
            .unwrap_or_else(|error| panic!("barrier fixture return must generate: {error}"));

        module
            .verify()
            .unwrap_or_else(|error| panic!("performance barrier module must verify: {error}"));

        let ir = module.print_to_string().to_string();

        let barrier = ir
            .lines()
            .find(|line| line.contains("asm sideeffect") && line.contains("~{memory}"))
            .unwrap_or_else(|| panic!("performance barrier must remain in generated LLVM"));

        assert!(barrier.contains("i64 %0"));
    }
}
