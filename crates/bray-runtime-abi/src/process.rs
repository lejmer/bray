use crate::{NativePlatformEnvironmentList, NativePlatformSpanList, NativePlatformText};

/// Complete call-only child-process construction request.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct NativePlatformChildRequest {
    executable: NativePlatformText,
    working_directory: NativePlatformText,
    arguments: NativePlatformSpanList,
    environment: NativePlatformEnvironmentList,
    standard_input: u32,
    standard_output: u32,
    standard_error: u32,
    reserved: u32,
}

impl NativePlatformChildRequest {
    /// Creates one complete child-process request.
    #[expect(
        clippy::too_many_arguments,
        reason = "the constructor mirrors the fixed native ABI record"
    )]
    pub const fn new(
        executable: NativePlatformText,
        working_directory: NativePlatformText,
        arguments: NativePlatformSpanList,
        environment: NativePlatformEnvironmentList,
        standard_input: u32,
        standard_output: u32,
        standard_error: u32,
    ) -> Self {
        Self {
            executable,
            working_directory,
            arguments,
            environment,
            standard_input,
            standard_output,
            standard_error,
            reserved: 0,
        }
    }

    /// Returns the executable path span.
    pub const fn executable(self) -> NativePlatformText {
        self.executable
    }

    /// Returns the optional working-directory path span.
    pub const fn working_directory(self) -> NativePlatformText {
        self.working_directory
    }

    /// Returns the ordered argument spans.
    pub const fn arguments(self) -> NativePlatformSpanList {
        self.arguments
    }

    /// Returns the complete environment entries.
    pub const fn environment(self) -> NativePlatformEnvironmentList {
        self.environment
    }

    /// Returns the standard-input policy ordinal.
    pub const fn standard_input(self) -> u32 {
        self.standard_input
    }

    /// Returns the standard-output policy ordinal.
    pub const fn standard_output(self) -> u32 {
        self.standard_output
    }

    /// Returns the standard-error policy ordinal.
    pub const fn standard_error(self) -> u32 {
        self.standard_error
    }

    /// Returns the reserved field, which must be zero.
    pub const fn reserved(self) -> u32 {
        self.reserved
    }
}

/// Native child-process exit status.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct NativePlatformExitStatus {
    tag: u32,
    reserved: u32,
    payload: i64,
}

impl NativePlatformExitStatus {
    /// Creates one portable exit-code status.
    pub const fn code(code: i32) -> Self {
        Self {
            tag: 0,
            reserved: 0,
            payload: code as i64,
        }
    }

    /// Creates one target termination status.
    pub const fn target_termination(code: i64) -> Self {
        Self {
            tag: 1,
            reserved: 0,
            payload: code,
        }
    }

    /// Returns the status variant ordinal.
    pub const fn tag(self) -> u32 {
        self.tag
    }

    /// Returns the variant payload.
    pub const fn payload(self) -> i64 {
        self.payload
    }
}

#[cfg(test)]
mod tests {
    use super::{NativePlatformChildRequest, NativePlatformExitStatus};

    #[test]
    fn process_records_have_the_native_abi_layout() {
        assert_abi_layout!(NativePlatformChildRequest, size: 80, align: 8, fields: {
            executable: 0,
            working_directory: 16,
            arguments: 32,
            environment: 48,
            standard_input: 64,
            standard_output: 68,
            standard_error: 72,
            reserved: 76,
        });

        assert_abi_layout!(NativePlatformExitStatus, size: 16, align: 8, fields: {
            tag: 0,
            reserved: 4,
            payload: 8,
        });
    }
}
