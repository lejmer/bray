use std::any::Any;

/// Returns the text carried by a caught panic.
pub fn panic_payload_text(panic: &(dyn Any + Send)) -> &str {
    panic
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| panic.downcast_ref::<&str>().copied())
        .expect("panic payload must be text")
}
