//! Resident runtime callbacks shared by every provider in one linked product.

use crate::{
    NativeExecutionLaneResult, NativeFrameProgress, NativeInactiveFrame, NativePanicReport,
    NativeProductHostDescriptor, NativeProductHostObservation, NativeProductHostOperation,
    NativeProtectedFrameTransfer, NativeRootHandle, NativeRootStart, NativeRunOutcome,
    NativeRunResultLayout, NativeRuntimeConfiguration, NativeRuntimeEventCallback,
    NativeRuntimeStatus, NativeSynchronousRootCallback, NativeTaskAllocation, NativeTaskHandle,
    NativeThreadCancellationCallback, NativeThreadOperationCallback,
    NativeThreadStaticCleanupRegistration, NativeWakeCallback,
};

type NativeValueCleanupCallback = extern "C-unwind" fn(*mut u8);

macro_rules! define_resident_services {
    ($( $role:ident {
        $documentation:literal, $name:literal,
        native: ($($native:tt)*),
        $(resident: ($resident_service:ident $resident_field:ident: $resident_callback:ty),)?
        call_hook: ($($hook:ident)?),
        compiler: $abi:ident [$($parameter:ident),*] -> $result:ident,
        owner: $owner:ident, availability: $availability:ident,
        bootstrap: ($($bootstrap:literal)?), host_control: $host_control:literal,
        capabilities: [$($capability:ident),*],
        effects: [$($effect:ident),*]
    })+) => {
        $(define_resident_services!(@requires_native ($($native)*) $($resident_service)?);)+
        define_resident_services! {
            @collect [] [];
            $(( $($resident_service $resident_field: $resident_callback)? ))+
        }
    };
    (@requires_native () Host) => {
        compile_error!("a resident host service must have a native signature");
    };
    (@requires_native () Execution) => {
        compile_error!("a resident execution service must have a native signature");
    };
    (@requires_native ($($native:tt)+) $resident_service:ident) => {};
    (@requires_native ($($native:tt)*) ) => {};
    (@collect [$($host:tt)*] [$($execution:tt)*]; (Host $field:ident: $callback:ty) $($rest:tt)*) => {
        define_resident_services!(@collect [$($host)* $field: $callback,] [$($execution)*]; $($rest)*);
    };
    (@collect [$($host:tt)*] [$($execution:tt)*]; (Execution $field:ident: $callback:ty) $($rest:tt)*) => {
        define_resident_services!(@collect [$($host)*] [$($execution)* $field: $callback,]; $($rest)*);
    };
    (@collect [$($host:tt)*] [$($execution:tt)*]; () $($rest:tt)*) => {
        define_resident_services!(@collect [$($host)*] [$($execution)*]; $($rest)*);
    };
    (@collect [$($host_field:ident: $host_callback:ty,)*] [$($execution_field:ident: $execution_callback:ty,)*];) => {
        /// Resident operations that do not require a scheduler.
        #[derive(Clone, Copy, Debug)]
        #[repr(C)]
        pub struct NativeHostServices {
            $(
                #[doc = concat!("Resident host callback `", stringify!($host_field), "`.")]
                pub $host_field: $host_callback,
            )*
        }

        /// Resident operations that require an independent execution service.
        #[derive(Clone, Copy, Debug)]
        #[repr(C)]
        pub struct NativeExecutionServices {
            /// Host service used by this execution service.
            pub host: &'static NativeHostServices,
            $(
                #[doc = concat!("Resident execution callback `", stringify!($execution_field), "`.")]
                pub $execution_field: $execution_callback,
            )*
        }
    }
}

crate::runtime_role_catalog!(define_resident_services);

#[cfg(test)]
mod tests {
    use super::{NativeExecutionServices, NativeHostServices};

    #[test]
    fn resident_service_tables_are_fixed_callback_arrays() {
        let word = std::mem::size_of::<usize>();

        assert_eq!(std::mem::size_of::<NativeHostServices>(), 22 * word);
        assert_eq!(std::mem::size_of::<NativeExecutionServices>(), 23 * word);
    }
}
