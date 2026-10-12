# Closed operation coverage inventory

Prepared on 2026-10-12. Companion to [the rule coverage and replacement plan](runtime-stack-rule-coverage.md).

These lists were mechanically extracted from the inspected repair tree and assigned to rules R01-R32. Each name occurs
exactly once within its inventory. This verifies accounting, not semantic correctness. The classifications identify the
owning behavior. They do not mean that every operation in a group has identical effects or is currently implemented
correctly. Numerical behavior, memory order, failure cases and copy modes retain their selected contracts.

The source inventories are BoundExpression, BoundStructuredExpressionKind, AnalysisOperationKind, AnalysisEdgeKind,
AnalysisExitKind, StorageIdentity, StorageProjection, StorageAccessPurpose, CheckedMemoryOperationKind,
ImplementationHook and runtime_role_catalog. The main plan links their source owners.

## Expressions (26)

| Behavior owner | Rules | Complete variant list |
| --- | --- | --- |
| Value and storage observation | R03, R06, R07 | `Literal`, `Name`, `PatternReference`, `MemberAccess`, `LeadingDotVariant`, `UnqualifiedVariant` |
| Selected operations and calls | R12, R13, R16 | `Unary`, `Binary`, `Call`, `Conversion`, `TraitQualifiedMember` |
| Owned construction and replacement | R01, R05, R11, R24 | `Assignment`, `BoxConstruction`, `StructConstruction` |
| Control and deferred execution | R14, R15, R16, R19, R20, R22, R23 | `Block`, `AnonymousCallable`, `Await`, `Structured`, `ControlTransfer`, `For`, `Match`, `Generator` |
| Earlier-error recovery | R30 | `UnresolvedReference`, `ErrorCall`, `ErrorConversion`, `Error` |

## Structured (26)

| Behavior owner | Rules | Complete variant list |
| --- | --- | --- |
| Values, aggregates and copying | R01, R03, R06 | `Unit`, `Absence`, `Tuple`, `Array`, `RepeatedArray`, `Range` |
| Access and borrowing | R07, R08, R09, R13 | `ElementIndex`, `SliceIndex`, `Borrow` |
| Branches and loops | R06, R14, R15 | `Conditional`, `PatternTest`, `PatternBinding`, `Condition`, `While`, `Loop`, `BooleanAllFold`, `BooleanAnyFold` |
| Generator ownership | R01, R16, R22, R24 | `ArrayGenerator`, `GeneralGenerator` |
| Scope and trusted authority | R12, R19, R20, R27 | `With`, `TrustBoundary`, `Assertion` |
| Propagation and abnormal outcomes | R16, R20, R21 | `NullablePropagation`, `ResultPropagation`, `Catch`, `Panic` |

## Operations (7)

| Behavior owner | Rules | Complete variant list |
| --- | --- | --- |
| Selected source/implicit storage work | R01-R18, R27 | `Bound` |
| Observation with no acquisition | R06, R14 | `PatternObservation` |
| Attempt and completion | R12, R13 | `Call` |
| Await/yield/scoped suspension | R22, R23 | `Suspension` |
| Start/join/cancel phase transfer | R22, R23, R24 | `TaskOperation` |
| Broadcast then lifecycle | R19, R20, R21 | `ScopeExit` |
| Earlier-error recovery | R30 | `Recovery` |

## Edges (26)

| Behavior owner | Rules | Complete variant list |
| --- | --- | --- |
| Sequential transfer | R01-R13 | `Sequential` |
| Guarded alternatives | R06, R14 | `ConditionalTrue`, `ConditionalFalse`, `MatchArm`, `MatchNoMatch`, `NullablePresent`, `NullableAbsent` |
| Loop ownership | R15, R20 | `LoopEntry`, `LoopBack`, `LoopBreak`, `LoopContinue` |
| Value and result transfer | R16, R20 | `Return`, `ResultSuccess`, `ResultErrorPropagation`, `RunResultCompleted`, `Yield` |
| Abnormal outcome | R20, R21 | `RunResultPanicked`, `RunResultCancelled`, `Catch`, `Panic`, `RunCancellation` |
| Deferred and scope transitions | R19, R20, R22, R23 | `Suspension`, `Resume`, `ScopeExit` |
| No continuing state | R15, R25 | `Divergence` |
| Earlier-error recovery | R30 | `Recovery` |

## Exits (8)

| Behavior owner | Rules | Complete variant list |
| --- | --- | --- |
| Ordinary ownership boundary | R16, R20 | `NormalFallthrough`, `Return`, `ResultErrorPropagation`, `Yield` |
| Abnormal cleanup | R21 | `Panic`, `Cancellation` |
| No fabricated termination proof | R15, R25 | `Divergence` |
| Earlier-error recovery | R30 | `Recovery` |

## Identities (17)

| Behavior owner | Rules | Complete variant list |
| --- | --- | --- |
| Owned local/component/result roots | R01, R04, R05, R16 | `LocalOwned`, `Result`, `Temporary`, `IterationCursor`, `IterationElement`, `Allocation`, `CompilerCreated` |
| Entry and contract roots | R12, R25, R29 | `Parameter`, `Receiver`, `AnonymousParameter`, `PredicateParameter`, `PostconditionResult` |
| Realized product or attachment root | R26, R28 | `Static` |
| Selected borrow/capability/alternative | R06, R07, R08, R19 | `CustomIndexBorrow`, `ScopedCapability`, `Alternative` |
| Earlier-error recovery | R30 | `Error` |

## Projections (9)

| Behavior owner | Rules | Complete variant list |
| --- | --- | --- |
| Initialized represented components | R01, R04, R06 | `ProductField`, `TupleElement`, `ElementFromStart`, `ElementFromEnd`, `ActiveUnionPayloadField`, `NullableValue` |
| Indexed or bounded range | R01, R07, R08, R10 | `Element`, `SliceRange` |
| Storage-policy target and retained owner | R07, R16, R24 | `OwnedTarget` |

## Accesses (12)

| Behavior owner | Rules | Complete variant list |
| --- | --- | --- |
| Observation or selected copy | R03, R06 | `Read`, `Copy`, `Member`, `Index`, `Slice`, `Projection` |
| Commit and consume | R01, R03, R05 | `Initialize`, `Move`, `ValueTransfer` |
| Borrow and replacement | R07, R10, R11 | `Borrow`, `Write`, `Assignment` |

## Memory (59)

| Behavior owner | Rules | Complete variant list |
| --- | --- | --- |
| Protected initialization | R02 | `UninitNew`, `UninitFrom`, `UninitPointer`, `UninitWrite`, `UninitAssumeInitialized`, `UninitMove` |
| Anchored reference and addresses | R07, R08, R09, R27, R28 | `BorrowFrom`, `Address`, `Null`, `IsNull`, `Offset`, `Reinterpret`, `CallableFromPointer`, `PointerFromCallable`, `CallbackState` |
| Contents and copy/move | R02, R03, R10 | `Read`, `Write`, `Copy`, `ByteBufferFill`, `ByteBufferRead` |
| Layout and allocation lifetime | R09, R24 | `LayoutQuery`, `RawAllocate`, `RawDeallocate`, `Allocate`, `Deallocate` |
| Owned buffer lifecycle | R01, R02, R03, R11, R24 | `RawBufferAllocate`, `RawBufferPush`, `RawBufferPop`, `RawBufferSetInitializedCount`, `RawBufferRelease`, `RawBufferReplace`, `RawBufferRelocate` |
| Buffer observation and views | R06, R07, R08, R09 | `RawBufferCapacity`, `RawBufferInitializedCount`, `RawBufferPointer`, `RawBufferInitializedSlice`, `RawBufferInitializedSliceMut`, `RawBufferSparePointer`, `SequenceLength` |
| Volatile and provenance-free addresses | R09, R10, R27 | `VolatileRead`, `VolatileWrite`, `ExposeAddress`, `FromExposedAddress`, `CompareAddress` |
| Target control and stated assembly effects | R12, R21, R25, R27 | `Fence`, `CatastrophicAbort`, `DebuggerTrap`, `UnreachableTermination`, `SpinLoopHint`, `TargetFeatureEnabled`, `InlineAssembly` |
| Atomic operation and ordering | R10, R12, R23, R27 | `AtomicInitialize`, `AtomicLoad`, `AtomicStore`, `AtomicExchange`, `AtomicCompareExchange`, `AtomicFetch`, `AtomicWait`, `AtomicNotify` |

## Hooks (123)

| Behavior owner | Rules | Complete variant list |
| --- | --- | --- |
| Address and anchored reference | R07, R08, R09, R27, R28 | `AddressOf`, `AddressOfMut`, `RawPointerNull`, `RawPointerIsNull`, `RawPointerOffset`, `RawPointerByteOffset`, `RawPointerReinterpret`, `CallableFromPointer`, `PointerFromCallable`, `BorrowFrom`, `BorrowMutFrom`, `CallbackState`, `TransferredValueBorrow` |
| Contents and protected storage | R02, R03, R10 | `RawPointerRead`, `RawPointerWrite`, `MemoryCopy`, `MemoryCopyOverlapping`, `UninitNew`, `UninitFrom`, `UninitPointer`, `UninitPointerMut`, `UninitWrite`, `UninitAssumeInitialized`, `UninitMove`, `ByteBufferFill`, `ByteBufferRead` |
| Layout and allocation | R09, R24 | `MemorySizeOf`, `MemoryAlignOf`, `MemoryStrideOf`, `MemoryLayoutOf`, `MemoryTrailingLayoutOf`, `RawAllocate`, `RawDeallocate`, `Allocate`, `Deallocate` |
| Buffer ownership | R01, R02, R03, R11, R24 | `RawBufferAllocate`, `RawBufferPush`, `RawBufferPop`, `RawBufferSetInitializedCount`, `RawBufferRelease`, `RawBufferReplace`, `RawBufferRelocate` |
| Buffer and presence observations | R06, R07, R08, R09 | `RawBufferCapacity`, `RawBufferInitializedCount`, `RawBufferPointer`, `RawBufferInitializedSlice`, `RawBufferInitializedSliceMut`, `RawBufferSparePointer`, `SequenceLength`, `SequenceIsEmpty`, `NullableIsPresent`, `NullableIsAbsent` |
| Volatile and numerical address exposure | R09, R10, R27 | `VolatileLoad`, `VolatileStore`, `DeviceVolatileLoad`, `DeviceVolatileStore`, `PointerExposeAddress`, `PointerFromExposedAddress`, `PointerAddressEqual`, `PointerAddressLess` |
| Target control and assembly | R12, R21, R25, R27 | `CompilerFence`, `HardwareFence`, `CatastrophicAbort`, `DebuggerTrap`, `UnreachableTermination`, `SpinLoopHint`, `TargetFeatureEnabled`, `InlineAssembly`, `DivergingInlineAssembly`, `BranchingInlineAssembly` |
| Atomic operation and ordering | R10, R12, R23, R27 | `AtomicInitialize`, `AtomicLoad`, `AtomicStore`, `AtomicExchange`, `AtomicCompareExchange`, `AtomicCompareExchangeWeak`, `AtomicFetchAdd`, `AtomicFetchSub`, `AtomicFetchAnd`, `AtomicFetchOr`, `AtomicFetchXor`, `AtomicFence`, `AtomicCompilerFence`, `AtomicWait`, `AtomicNotifyOne`, `AtomicNotifyAll` |
| Run, task and execution authority | R12, R22, R23, R24 | `FutureStart`, `TaskJoin`, `TaskCancel`, `BlockingExecution`, `ComputeExecution`, `MainThreadExecution`, `CurrentRunCancellationObservation`, `CurrentRunCancellationPropagation`, `NativeThreadExecution`, `NativeThreadStart`, `CurrentNativeThreadIdentity`, `MainNativeThreadIdentity`, `NativeThreadPanicReporting`, `TaskEventCreation`, `TaskEventSignal`, `TaskEventDestruction`, `TaskEventWait`, `TaskYield` |
| String, character and numeric selected behavior | R03, R06, R12, R13 | `StringScalarCount`, `StringIsEmpty`, `StringEquals`, `StringScalarAt`, `StringScalarSlice`, `StringUtf8`, `StringFromUtf8`, `CharacterScalarValue`, `CharacterFromScalarValue`, `CharacterUtf8Length`, `CharacterUtf8Byte`, `CharacterIsAlphabetic`, `CharacterIsNumeric`, `CharacterIsWhitespace`, `NumericTruncate` |
| Iteration and test failure | R12, R13, R15, R21 | `RangeSharedIterate`, `RangeMoveIterate`, `RangeNext`, `TestingFail` |

## Roles (79)

| Behavior owner | Rules | Complete variant list |
| --- | --- | --- |
| Existing report records and observation | R05, R21, R24, R28, R31 | `ReportRecordAdmission`, `ReportRecordTake`, `ReportRecordExchange`, `ReportRecordAppend`, `ReportRecordPop`, `ReportSegmentMark`, `ReportSegmentTake`, `ReportConsumer`, `MemoryObservationBegin`, `MemoryAllocationObservation`, `MemoryCopyObservation`, `PerformanceIntervalBegin`, `PerformanceIntervalEnd` |
| Host and entry | R12, R21, R26, R27, R28 | `RuntimeInitialization`, `SynchronousRootExecution`, `ForeignCallbackExecution`, `NativeThreadExecution`, `ProductHostControl` |
| Attachment and TLS | R26, R27, R28 | `ThreadAttachmentEnter`, `ThreadAttachmentLeave`, `ThreadAttachmentMarkMain`, `ThreadAttachmentCleanup`, `ThreadStorageExit`, `CurrentNativeThreadIdentity`, `MainNativeThreadIdentity`, `ThreadAttachmentIdentity`, `ThreadStaticCleanupRegistration` |
| Run cancellation and shield | R21, R22, R23 | `RootCancellationRequest`, `TaskCancellationRequest`, `CurrentRunCancellationObservation`, `CurrentRunCancellationPropagation`, `CleanupShieldEnter`, `CleanupShieldLeave` |
| Events, registration and dispatch | R22, R23, R24 | `TaskEventCreation`, `TaskEventSignal`, `TaskEventDestruction`, `SuspensionRegistration`, `Wake`, `JoinRegistration`, `RuntimeEvent`, `CompatibleLaneSelection` |
| Task and observation owner | R22, R23, R24 | `TaskAllocation`, `TaskStart`, `TaskObservationCreation`, `TaskResolution`, `TerminalPublication`, `TaskDestruction` |
| Activation and generated frame work | R01, R05, R20, R22, R23, R24 | `FrameResume`, `FrameTaskBroadcast`, `FrameLifecycleResolution`, `FrameCompletionMove`, `FrameDestruction`, `FrameCreation`, `InactiveFrameMove`, `AwaitedFrameComposition` |
| Root and terminal host | R20, R21, R26, R28 | `RootExecution`, `MainThreadLaneStartup`, `MainThreadLaneDrive`, `RootTerminalObservation`, `RootCompletionResolution`, `StructuredShutdown` |
| Typed incidents, report and outgoing backing | R21, R24, R28, R32 | `CleanupIncidentTransfer`, `CleanupIncidentReporting`, `PanicReporting`, `PanicReportDestruction`, `OutgoingAdmission`, `OutgoingDischarge`, `OutgoingActivation`, `OutgoingRetirement`, `PanicReportSuppression`, `EntryFailureReporting`, `PanicReportConstruction`, `PanicPropagation` |
| Generators | R01, R16, R20, R24 | `GeneratorBegin`, `GeneratorPush`, `GeneratorFinish`, `GeneratorCleanupBroadcast`, `GeneratorDestruction` |
| Test product selection | R21, R26, R31 | `TestEntrySelection` |



## Platform roles (69)

The platform catalog is an ABI inventory, not proof that a linked implementation is Bray. Existing direct Bray target
bindings should be reused. Apply these ownership/output rules to those bindings and remove remaining linked Rust routes
under the active product migration owners. The documented temporal-provider exception remains the stated exception.
No closed platform issue is reopened.

| Behavior owner | Rules | Complete variant list |
| --- | --- | --- |
| Process context | R09, R26, R27 | `ContextIdentity`, `ContextNativeTextWidth`, `ContextWorkingDirectory`, `ContextArgumentCount`, `ContextArgument`, `ContextEnvironmentCount`, `ContextEnvironmentEntry`, `ContextEnvironmentKeyEquals` |
| Streams, files and filesystem | R01, R10, R12, R17, R27 | `StandardInputRead`, `StandardInputLock`, `StandardInputUnlock`, `StandardOutputWrite`, `StandardOutputFlush`, `StandardOutputLock`, `StandardOutputUnlock`, `StandardErrorWrite`, `StandardErrorFlush`, `StandardErrorLock`, `StandardErrorUnlock`, `FileRead`, `FileWrite`, `FileFlush`, `FileSeek`, `FileClose`, `FileOpen`, `FileMetadata`, `PathMetadata`, `DirectoryOpen`, `DirectoryNext`, `DirectoryClose`, `PathCreateDirectory`, `PathRemoveFile`, `PathRemoveDirectory`, `PathRename` |
| Child process and pipes | R01, R17, R21, R23, R24, R27 | `ProcessPipeRead`, `ProcessPipeWrite`, `ProcessPipeFlush`, `ProcessPipeClose`, `ChildSpawn`, `ChildWait`, `ChildTerminate`, `ChildReap`, `ChildDispose` |
| Native thread and TLS | R23, R26, R27 | `ThreadCreate`, `ThreadJoin`, `ThreadDetach`, `ThreadStorageCreate`, `ThreadStorageLoad`, `ThreadStorageStore`, `ThreadStorageDestroy` |
| Clock and entropy | R10, R12, R23, R27 | `ClockMonotonicNow`, `ClockWallNow`, `ClockSleep`, `EntropyFill` |
| Temporal provider and owned results | R01, R12, R17, R27, R28, R31 | `TimeDateValidate`, `TimeDateAdd`, `TimeZoneLoad`, `TimeZoneLocal`, `TimeZoneRetain`, `TimeZoneClose`, `TimeZoneName`, `TimeObserve`, `TimeResolve`, `TimeParse`, `TimeFormat` |
| Dynamic provider | R09, R26, R27, R28 | `DynamicLibraryOpenPath`, `DynamicLibraryOpenSystem`, `DynamicLibrarySymbol`, `DynamicLibraryClose` |


## Source inventory links

- [Bound expressions](../../crates/bray-bound-tree/src/node/expression/core.rs)
- [Structured expressions](../../crates/bray-bound-tree/src/node/expression/category.rs)
- [Analysis operations, edges and exits](../../crates/bray-checker/src/analysis/model.rs)
- [Storage identities and projections](../../crates/bray-bound-tree/src/storage/model.rs)
- [Storage access purposes](../../crates/bray-bound-tree/src/storage/plan/contracts.rs)
- [Checked memory operations](../../crates/bray-bound-tree/src/memory.rs)
- [Compiler-known hooks](../../crates/bray-compiler-known/src/implementation.rs)
- [Runtime execution roles](../../crates/bray-runtime-abi/src/catalog/execution.rs)
- [Platform roles](../../crates/bray-runtime-abi/src/catalog/platform.rs)

Mechanical accounting covers 461 entries across the 12 inventories. Every entry is assigned exactly once within its
inventory. This is a finite coverage boundary for the assessment, not a claim of 461 independent semantic proofs.
