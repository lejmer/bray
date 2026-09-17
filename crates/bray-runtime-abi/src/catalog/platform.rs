/// Expands the complete platform-service role catalog for one representation.
#[macro_export]
macro_rules! platform_role_catalog {
    ($projection:ident) => {
        $projection! {
            ContextIdentity {
                "Reads the immutable process identity.", 0x0001, "platform.context.identity",
                PLATFORM_CONTEXT_IDENTITY_SYMBOL = "bray_platform_context_identity",
                Core, [] -> U64, bootstrap: ()
            }
            ContextNativeTextWidth {
                "Reads the target-native text width.", 0x0002, "platform.context.native_text_width",
                PLATFORM_CONTEXT_NATIVE_TEXT_WIDTH_SYMBOL = "bray_platform_context_native_text_width",
                Core, [] -> U32, bootstrap: ()
            }
            ContextWorkingDirectory {
                "Borrows the startup working directory.", 0x0003, "platform.context.working_directory",
                PLATFORM_CONTEXT_WORKING_DIRECTORY_SYMBOL = "bray_platform_context_working_directory",
                Core, [RawAddressPointer, PointerU64] -> Status, bootstrap: ()
            }
            ContextArgumentCount {
                "Reads the startup argument count.", 0x0004, "platform.context.argument_count",
                PLATFORM_CONTEXT_ARGUMENT_COUNT_SYMBOL = "bray_platform_context_argument_count",
                Core, [PointerU64] -> Status, bootstrap: ()
            }
            ContextArgument {
                "Borrows one startup argument.", 0x0005, "platform.context.argument",
                PLATFORM_CONTEXT_ARGUMENT_SYMBOL = "bray_platform_context_argument",
                Core, [U64, RawAddressPointer, PointerU64] -> Status, bootstrap: ()
            }
            ContextEnvironmentCount {
                "Reads the startup environment-entry count.", 0x0006, "platform.context.environment_count",
                PLATFORM_CONTEXT_ENVIRONMENT_COUNT_SYMBOL = "bray_platform_context_environment_count",
                Core, [PointerU64] -> Status, bootstrap: ()
            }
            ContextEnvironmentEntry {
                "Borrows one startup environment entry.", 0x0007, "platform.context.environment_entry",
                PLATFORM_CONTEXT_ENVIRONMENT_ENTRY_SYMBOL = "bray_platform_context_environment_entry",
                Core, [U64, RawAddressPointer, PointerU64, RawAddressPointer, PointerU64] -> Status, bootstrap: ()
            }
            ContextEnvironmentKeyEquals {
                "Compares environment keys using the target's process-environment rules.", 0x0008, "platform.context.environment_key_equals",
                PLATFORM_CONTEXT_ENVIRONMENT_KEY_EQUALS_SYMBOL = "bray_platform_context_environment_key_equals",
                Core, [NativeText, NativeText, PointerU32] -> Status, bootstrap: ()
            }
            StandardInputRead {
                "Reads bytes from standard input.", 0x0101, "platform.standard_input.read",
                PLATFORM_STANDARD_INPUT_READ_SYMBOL = "bray_platform_standard_input_read",
                StandardStreams, [PointerU8, U64, PointerU64] -> Status, bootstrap: ()
            }
            StandardInputLock {
                "Acquires product-wide standard-input serialization.", 0x0102, "platform.standard_input.lock",
                PLATFORM_STANDARD_INPUT_LOCK_SYMBOL = "bray_platform_standard_input_lock",
                StandardStreams, [] -> Status, bootstrap: ()
            }
            StandardInputUnlock {
                "Releases product-wide standard-input serialization.", 0x0103, "platform.standard_input.unlock",
                PLATFORM_STANDARD_INPUT_UNLOCK_SYMBOL = "bray_platform_standard_input_unlock",
                StandardStreams, [] -> Status, bootstrap: ()
            }
            StandardOutputWrite {
                "Writes bytes to standard output.", 0x0111, "platform.standard_output.write",
                PLATFORM_STANDARD_OUTPUT_WRITE_SYMBOL = "bray_platform_standard_output_write",
                StandardStreams, [PointerU8, U64, PointerU64] -> Status, bootstrap: ()
            }
            StandardOutputFlush {
                "Flushes standard output.", 0x0112, "platform.standard_output.flush",
                PLATFORM_STANDARD_OUTPUT_FLUSH_SYMBOL = "bray_platform_standard_output_flush",
                StandardStreams, [] -> Status, bootstrap: ()
            }
            StandardOutputLock {
                "Acquires product-wide standard-output serialization.", 0x0113, "platform.standard_output.lock",
                PLATFORM_STANDARD_OUTPUT_LOCK_SYMBOL = "bray_platform_standard_output_lock",
                StandardStreams, [] -> Status, bootstrap: ()
            }
            StandardOutputUnlock {
                "Releases product-wide standard-output serialization.", 0x0114, "platform.standard_output.unlock",
                PLATFORM_STANDARD_OUTPUT_UNLOCK_SYMBOL = "bray_platform_standard_output_unlock",
                StandardStreams, [] -> Status, bootstrap: ()
            }
            StandardErrorWrite {
                "Writes bytes to standard error.", 0x0121, "platform.standard_error.write",
                PLATFORM_STANDARD_ERROR_WRITE_SYMBOL = "bray_platform_standard_error_write",
                StandardStreams, [PointerU8, U64, PointerU64] -> Status, bootstrap: ()
            }
            StandardErrorFlush {
                "Flushes standard error.", 0x0122, "platform.standard_error.flush",
                PLATFORM_STANDARD_ERROR_FLUSH_SYMBOL = "bray_platform_standard_error_flush",
                StandardStreams, [] -> Status, bootstrap: ()
            }
            StandardErrorLock {
                "Acquires product-wide standard-error serialization.", 0x0123, "platform.standard_error.lock",
                PLATFORM_STANDARD_ERROR_LOCK_SYMBOL = "bray_platform_standard_error_lock",
                StandardStreams, [] -> Status, bootstrap: ()
            }
            StandardErrorUnlock {
                "Releases product-wide standard-error serialization.", 0x0124, "platform.standard_error.unlock",
                PLATFORM_STANDARD_ERROR_UNLOCK_SYMBOL = "bray_platform_standard_error_unlock",
                StandardStreams, [] -> Status, bootstrap: ()
            }
            FileRead {
                "Reads bytes from a borrowed file owner.", 0x0201, "platform.file.read",
                PLATFORM_FILE_READ_SYMBOL = "bray_platform_file_read",
                Filesystem, [U64, PointerU8, U64, PointerU64] -> Status, bootstrap: ()
            }
            FileWrite {
                "Writes bytes to a borrowed file owner.", 0x0202, "platform.file.write",
                PLATFORM_FILE_WRITE_SYMBOL = "bray_platform_file_write",
                Filesystem, [U64, PointerU8, U64, PointerU64] -> Status, bootstrap: ()
            }
            FileFlush {
                "Flushes a borrowed file owner.", 0x0203, "platform.file.flush",
                PLATFORM_FILE_FLUSH_SYMBOL = "bray_platform_file_flush",
                Filesystem, [U64] -> Status, bootstrap: ()
            }
            FileSeek {
                "Seeks a borrowed file owner.", 0x0204, "platform.file.seek",
                PLATFORM_FILE_SEEK_SYMBOL = "bray_platform_file_seek",
                Filesystem, [U64, U64, U32, PointerU64] -> Status, bootstrap: ()
            }
            FileClose {
                "Closes an owned file owner.", 0x0205, "platform.file.close",
                PLATFORM_FILE_CLOSE_SYMBOL = "bray_platform_file_close",
                Filesystem, [U64] -> Status, bootstrap: ()
            }
            FileOpen {
                "Opens one file stream.", 0x0211, "platform.file.open",
                PLATFORM_FILE_OPEN_SYMBOL = "bray_platform_file_open",
                Filesystem, [Path, FileOptions, PointerU64] -> Status, bootstrap: ()
            }
            FileMetadata {
                "Reads metadata from a borrowed file stream.", 0x0212, "platform.file.metadata",
                PLATFORM_FILE_METADATA_SYMBOL = "bray_platform_file_metadata",
                Filesystem, [U64, FileMetadataPointer] -> Status, bootstrap: ()
            }
            PathMetadata {
                "Reads metadata for one path.", 0x0213, "platform.path.metadata",
                PLATFORM_PATH_METADATA_SYMBOL = "bray_platform_path_metadata",
                Filesystem, [Path, FileMetadataPointer] -> Status, bootstrap: ()
            }
            DirectoryOpen {
                "Opens one directory traversal.", 0x0221, "platform.directory.open",
                PLATFORM_DIRECTORY_OPEN_SYMBOL = "bray_platform_directory_open",
                Filesystem, [Path, PointerU64] -> Status, bootstrap: ()
            }
            DirectoryNext {
                "Reads the next directory traversal entry.", 0x0222, "platform.directory.next",
                PLATFORM_DIRECTORY_NEXT_SYMBOL = "bray_platform_directory_next",
                Filesystem, [U64, PointerU8, U64, PointerU64, PointerU32, FileMetadataPointer] -> Status, bootstrap: ()
            }
            DirectoryClose {
                "Closes an owned directory traversal.", 0x0223, "platform.directory.close",
                PLATFORM_DIRECTORY_CLOSE_SYMBOL = "bray_platform_directory_close",
                Filesystem, [U64] -> Status, bootstrap: ()
            }
            PathCreateDirectory {
                "Creates one directory.", 0x0230, "platform.path.create_directory",
                PLATFORM_PATH_CREATE_DIRECTORY_SYMBOL = "bray_platform_path_create_directory",
                Filesystem, [Path] -> Status, bootstrap: ()
            }
            PathRemoveFile {
                "Removes one file.", 0x0231, "platform.path.remove_file",
                PLATFORM_PATH_REMOVE_FILE_SYMBOL = "bray_platform_path_remove_file",
                Filesystem, [Path] -> Status, bootstrap: ()
            }
            PathRemoveDirectory {
                "Removes one empty directory.", 0x0232, "platform.path.remove_directory",
                PLATFORM_PATH_REMOVE_DIRECTORY_SYMBOL = "bray_platform_path_remove_directory",
                Filesystem, [Path] -> Status, bootstrap: ()
            }
            PathRename {
                "Renames one filesystem entry.", 0x0233, "platform.path.rename",
                PLATFORM_PATH_RENAME_SYMBOL = "bray_platform_path_rename",
                Filesystem, [Path, Path] -> Status, bootstrap: ()
            }
            ProcessPipeRead {
                "Reads bytes from a borrowed child-process pipe owner.", 0x0301, "platform.process_pipe.read",
                PLATFORM_PROCESS_PIPE_READ_SYMBOL = "bray_platform_process_pipe_read",
                Process, [U64, PointerU8, U64, PointerU64] -> Status, bootstrap: ()
            }
            ProcessPipeWrite {
                "Writes bytes to a borrowed child-process pipe owner.", 0x0302, "platform.process_pipe.write",
                PLATFORM_PROCESS_PIPE_WRITE_SYMBOL = "bray_platform_process_pipe_write",
                Process, [U64, PointerU8, U64, PointerU64] -> Status, bootstrap: ()
            }
            ProcessPipeFlush {
                "Flushes a borrowed child-process pipe owner.", 0x0303, "platform.process_pipe.flush",
                PLATFORM_PROCESS_PIPE_FLUSH_SYMBOL = "bray_platform_process_pipe_flush",
                Process, [U64] -> Status, bootstrap: ()
            }
            ProcessPipeClose {
                "Closes an owned child-process pipe owner.", 0x0304, "platform.process_pipe.close",
                PLATFORM_PROCESS_PIPE_CLOSE_SYMBOL = "bray_platform_process_pipe_close",
                Process, [U64] -> Status, bootstrap: ()
            }
            ChildSpawn {
                "Creates one child process and its requested pipe owners.", 0x0311, "platform.child.spawn",
                PLATFORM_CHILD_SPAWN_SYMBOL = "bray_platform_child_spawn",
                Process, [ChildRequest, PointerU64, PointerU64, PointerU64, PointerU64] -> Status, bootstrap: ()
            }
            ChildWait {
                "Waits for a child to terminate without consuming its owner.", 0x0312, "platform.child.wait",
                PLATFORM_CHILD_WAIT_SYMBOL = "bray_platform_child_wait",
                Process, [U64, PointerU32, ExitStatusPointer] -> Status, bootstrap: ()
            }
            ChildTerminate {
                "Requests child-process termination without consuming its owner.", 0x0313, "platform.child.terminate",
                PLATFORM_CHILD_TERMINATE_SYMBOL = "bray_platform_child_terminate",
                Process, [U64, U32] -> Status, bootstrap: ()
            }
            ChildReap {
                "Consumes one terminal child-process owner.", 0x0314, "platform.child.reap",
                PLATFORM_CHILD_REAP_SYMBOL = "bray_platform_child_reap",
                Process, [U64, ExitStatusPointer] -> Status, bootstrap: ()
            }
            ChildDispose {
                "Forcefully resolves and consumes one child-process owner.", 0x0315, "platform.child.dispose",
                PLATFORM_CHILD_DISPOSE_SYMBOL = "bray_platform_child_dispose",
                Process, [U64] -> Status, bootstrap: ()
            }
            ThreadCreate {
                "Creates one operating-system thread and transfers its callback context.", 0x0321, "platform.thread.create",
                PLATFORM_THREAD_CREATE_SYMBOL = "bray_platform_thread_create",
                Thread, [PointerU8, PointerU8, PointerU64, PointerI64] -> Status, bootstrap: ()
            }
            ThreadJoin {
                "Waits for one operating-system thread and consumes its owner.", 0x0322, "platform.thread.join",
                PLATFORM_THREAD_JOIN_SYMBOL = "bray_platform_thread_join",
                Thread, [U64] -> Status, bootstrap: ()
            }
            ThreadDetach {
                "Releases the join authority for one operating-system thread.", 0x0323, "platform.thread.detach",
                PLATFORM_THREAD_DETACH_SYMBOL = "bray_platform_thread_detach",
                Thread, [U64] -> Status, bootstrap: ()
            }
            ThreadStorageCreate {
                "Creates one destructor-bearing native thread-storage key.", 0x0331, "platform.thread_storage.create",
                PLATFORM_THREAD_STORAGE_CREATE_SYMBOL = "bray_platform_thread_storage_create",
                Thread, [PointerU8, PointerU64] -> Status, bootstrap: ()
            }
            ThreadStorageLoad {
                "Loads the calling native thread's value for one storage key.", 0x0332, "platform.thread_storage.load",
                PLATFORM_THREAD_STORAGE_LOAD_SYMBOL = "bray_platform_thread_storage_load",
                Thread, [U64, RawAddressPointer] -> Status, bootstrap: ()
            }
            ThreadStorageStore {
                "Stores or clears the calling native thread's value for one storage key.", 0x0333, "platform.thread_storage.store",
                PLATFORM_THREAD_STORAGE_STORE_SYMBOL = "bray_platform_thread_storage_store",
                Thread, [U64, PointerU8] -> Status, bootstrap: ()
            }
            ThreadStorageDestroy {
                "Destroys one storage key after its attached threads have quiesced.", 0x0334, "platform.thread_storage.destroy",
                PLATFORM_THREAD_STORAGE_DESTROY_SYMBOL = "bray_platform_thread_storage_destroy",
                Thread, [U64] -> Status, bootstrap: ()
            }
            ClockMonotonicNow {
                "Observes the process-local monotonic clock.", 0x0401, "platform.clock.monotonic_now",
                PLATFORM_CLOCK_MONOTONIC_NOW_SYMBOL = "bray_platform_clock_monotonic_now",
                Core, [PointerU64] -> Status, bootstrap: ()
            }
            ClockWallNow {
                "Observes the host wall clock.", 0x0402, "platform.clock.wall_now",
                PLATFORM_CLOCK_WALL_NOW_SYMBOL = "bray_platform_clock_wall_now",
                Core, [PointerI64, PointerU32] -> Status, bootstrap: ()
            }
            ClockSleep {
                "Blocks the current native thread for a duration.", 0x0403, "platform.clock.sleep",
                PLATFORM_CLOCK_SLEEP_SYMBOL = "bray_platform_clock_sleep",
                Core, [U64, U32] -> Status, bootstrap: ()
            }
            EntropyFill {
                "Fills caller-owned bytes from the host entropy source.", 0x0501, "platform.entropy.fill",
                PLATFORM_ENTROPY_FILL_SYMBOL = "bray_platform_entropy_fill",
                Core, [PointerU8, U64, PointerU64] -> Status, bootstrap: ()
            }
            TimeDateValidate {
                "Validates one proleptic Gregorian date.", 0x0701, "platform.time.date_validate",
                PLATFORM_TIME_DATE_VALIDATE_SYMBOL = "bray_platform_time_date_validate",
                Temporal, [I32, U32, U32, PointerU32] -> Status, bootstrap: ()
            }
            TimeDateAdd {
                "Applies a checked calendar period to one date.", 0x0702, "platform.time.date_add",
                PLATFORM_TIME_DATE_ADD_SYMBOL = "bray_platform_time_date_add",
                Temporal, [TemporalDateTime, I32, I32, I32, U32, TemporalDateTimePointer, PointerU32] -> Status, bootstrap: ()
            }
            TimeZoneLoad {
                "Loads one named timezone from the pinned database.", 0x0710, "platform.time.zone_load",
                PLATFORM_TIME_ZONE_LOAD_SYMBOL = "bray_platform_time_zone_load",
                Temporal, [NativeText, PointerU64, PointerU32] -> Status, bootstrap: ()
            }
            TimeZoneLocal {
                "Discovers and loads the host's local timezone when available.", 0x0711, "platform.time.zone_local",
                PLATFORM_TIME_ZONE_LOCAL_SYMBOL = "bray_platform_time_zone_local",
                Temporal, [PointerU64, PointerU32] -> Status, bootstrap: ()
            }
            TimeZoneRetain {
                "Retains one immutable timezone owner.", 0x0712, "platform.time.zone_retain",
                PLATFORM_TIME_ZONE_RETAIN_SYMBOL = "bray_platform_time_zone_retain",
                Temporal, [U64] -> Status, bootstrap: ()
            }
            TimeZoneClose {
                "Releases one immutable timezone owner.", 0x0713, "platform.time.zone_close",
                PLATFORM_TIME_ZONE_CLOSE_SYMBOL = "bray_platform_time_zone_close",
                Temporal, [U64] -> Status, bootstrap: ()
            }
            TimeZoneName {
                "Copies one timezone's canonical IANA name.", 0x0714, "platform.time.zone_name",
                PLATFORM_TIME_ZONE_NAME_SYMBOL = "bray_platform_time_zone_name",
                Temporal, [U64, PointerU8, U64, PointerU64, PointerU32] -> Status, bootstrap: ()
            }
            TimeObserve {
                "Observes one timestamp through a named zone or fixed offset.", 0x0720, "platform.time.observe",
                PLATFORM_TIME_OBSERVE_SYMBOL = "bray_platform_time_observe",
                Temporal, [U64, I32, I64, U32, TemporalObservationPointer, PointerU8, U64, PointerU64, PointerU32] -> Status, bootstrap: ()
            }
            TimeResolve {
                "Resolves one local date-time through a named zone or fixed offset.", 0x0721, "platform.time.resolve",
                PLATFORM_TIME_RESOLVE_SYMBOL = "bray_platform_time_resolve",
                Temporal, [U64, I32, TemporalDateTime, TemporalResolutionPointer, PointerU32] -> Status, bootstrap: ()
            }
            TimeParse {
                "Parses one strict standard temporal representation.", 0x0730, "platform.time.parse",
                PLATFORM_TIME_PARSE_SYMBOL = "bray_platform_time_parse",
                Temporal, [U32, NativeText, TemporalValuePointer, PointerU64, PointerU32] -> Status, bootstrap: ()
            }
            TimeFormat {
                "Formats one strict standard temporal representation.", 0x0731, "platform.time.format",
                PLATFORM_TIME_FORMAT_SYMBOL = "bray_platform_time_format",
                Temporal, [U32, TemporalValue, PointerU8, U64, PointerU64, PointerU32] -> Status, bootstrap: ()
            }
            DynamicLibraryOpenPath {
                "Opens one dynamic library from an explicit path.", 0x0801, "platform.dynamic_library.open_path",
                PLATFORM_DYNAMIC_LIBRARY_OPEN_PATH_SYMBOL = "bray_platform_dynamic_library_open_path",
                DynamicLibrary, [Path, U32, PointerU64] -> Status, bootstrap: ()
            }
            DynamicLibraryOpenSystem {
                "Opens one target-defined system library.", 0x0802, "platform.dynamic_library.open_system",
                PLATFORM_DYNAMIC_LIBRARY_OPEN_SYSTEM_SYMBOL = "bray_platform_dynamic_library_open_system",
                DynamicLibrary, [U32, U32, PointerU64] -> Status, bootstrap: ()
            }
            DynamicLibrarySymbol {
                "Resolves one exact symbol from a borrowed dynamic library.", 0x0803, "platform.dynamic_library.symbol",
                PLATFORM_DYNAMIC_LIBRARY_SYMBOL_SYMBOL = "bray_platform_dynamic_library_symbol",
                DynamicLibrary, [U64, PointerU8, U64, RawAddressPointer] -> Status, bootstrap: ()
            }
            DynamicLibraryClose {
                "Closes one owned dynamic library.", 0x0804, "platform.dynamic_library.close",
                PLATFORM_DYNAMIC_LIBRARY_CLOSE_SYMBOL = "bray_platform_dynamic_library_close",
                DynamicLibrary, [U64] -> Status, bootstrap: ()
            }
        }
    };
}
