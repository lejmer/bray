use std::sync::Once;

use inkwell::targets::{InitializationConfig, Target};

static INITIALIZE_LLVM: Once = Once::new();

pub(crate) fn initialize() {
    INITIALIZE_LLVM.call_once(|| Target::initialize_all(&InitializationConfig::default()));
}

#[cfg(test)]
mod tests {
    use std::thread;

    use super::initialize;

    #[test]
    fn llvm_initialization_is_safe_from_concurrent_workers() {
        thread::scope(|scope| {
            for _ in 0..8 {
                scope.spawn(initialize);
            }
        });
    }
}
