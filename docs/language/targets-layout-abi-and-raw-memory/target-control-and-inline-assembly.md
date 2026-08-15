# Target control and inline assembly

This chapter defines Bray operations whose meaning depends on selected-target instructions or optimizer barriers. They are compiler-provided declarations rather than a second command language. Every operation is checked before MIR lowering and remains explicit in MIR and compiled package interfaces.

## Volatile access

`core.target.volatile_load` and `core.target.volatile_store` perform one host-address-space volatile access. Their raw-memory predicates establish validity, alignment, and initialization. A volatile read of a copyable value preserves initialization. A volatile read of a non-copyable value moves the value and consumes the source initialization fact. A volatile store establishes initialization only after the store completes.

`core.target.device_volatile_load` and `core.target.device_volatile_store` use `DevicePointer<T>`, which is not interchangeable with host `RawPointer<T>`. The selected code generation target assigns the device pointer's exact address-space identity. They require both `raw_memory` and `device_memory`. A target without a device address space rejects them before code generation.

Volatile access is not atomic access. It creates no synchronization edge and supplies no inter-thread ordering guarantee.

## Pointer addresses

`core.target.expose_address` converts a raw pointer to a target-width unsigned address and explicitly discards provenance. `core.target.from_exposed_address` reconstructs a raw pointer from such an address. Reconstruction does not establish validity, alignment, initialization, alias authority, allocation ownership, or a borrow dependency.

`core.target.address_equal` and `core.target.address_less` compare exposed target addresses. Equality and unsigned ordering are therefore address comparisons rather than provenance or allocation-identity comparisons. The operations are available only where target-width raw addresses are available.

## Barriers and termination

`core.target.compiler_fence` prevents the optimizer from moving memory effects across the fence. `core.target.hardware_fence` emits a target synchronization fence. Both accept `MemoryOrder`. Fence ordering must be `Acquire`, `Release`, `AcquireRelease`, or `SequentiallyConsistent`. `Relaxed` is rejected because it provides no meaningful fence contract. A compiler fence does not create a hardware synchronization edge.

`core.target.abort` performs catastrophic termination without source cleanup, panic propagation, or cancellation propagation. `core.target.debugger_trap` traps and may continue when a debugger resumes execution. `core.target.unreachable` terminates a trusted path whose reachability contract has been violated. MIR marks abort, unreachable, and diverging assembly as non-continuing control flow.

`core.target.spin_loop_hint` emits the selected architecture's non-synchronizing spin hint. It does not yield a Bray task and does not observe cancellation.

## Target facts and feature gates

The selected target profile determines its instruction set, guaranteed instruction features, register classes, physical registers, supported clobber ABIs, and inline-assembly availability. WebAssembly profiles reject inline assembly. `core.target.feature_enabled` accepts a string literal and returns whether that feature is guaranteed by the selected profile.

Assembly feature lists are comma-separated literal feature names. Every named feature must be guaranteed by the selected profile. Unsupported or dynamically computed feature requirements are rejected before MIR lowering.

## Trusted inline assembly

`core.target.assembly<Inputs, Outputs>`, `core.target.diverging_assembly<Inputs>`, and `core.target.branching_assembly<Inputs, Outputs, Labels>` are trusted compiler-provided declarations. Their template, constraint, clobber, feature, and option operands must be literals. Inputs and outputs are tuple types, including one-element tuples. Register and memory inputs are evaluated and moved according to their ordinary typed argument contracts. Immediate and callable-symbol inputs remain checked compile-time identities and do not become runtime tuple elements. A continuing assembly operation initializes its output tuple. A diverging operation has no output and no normal successor. Branching assembly also accepts a tuple of synchronous `func() -> never` label callables and is represented as an explicit MIR terminator with one normal result continuation.

The declarations require `intrinsic`, `raw_memory`, `device_memory`, `unchecked_alias`, and `unchecked_init`. This capability set makes potential register, memory, device, alias, and initialization effects visible in the containing trusted declaration. Ordinary source cannot invoke the operations without acknowledging those capabilities.

The constraint literal is a comma-separated list. A late output begins with `=`. An early output begins with `=&`. An input-output operand begins with `+`, with `+&` selecting an early input-output operand. Register classes, explicit physical registers, immediate `i`, symbol `s`, memory `m`, and `label` constraints are target checked against each corresponding tuple element before MIR lowering. Register operands accept only target-compatible scalar or pointer representations and widths. Immediates must be checked integer constants. Symbol operands must be direct callable names whose fully closed instance is retained for relocation. Memory operands must be pointer values. Labels must be synchronous `func() -> never` values.

Output-bearing constraints come first, followed by pure inputs and labels. Output tuple elements, input tuple elements, and label tuple elements each follow their own descriptor order. An input-output descriptor contributes one output and one tied input. LLVM template positions follow lowered constraint positions, so a `+reg,reg` contract uses `$0` for the output, `$1` for its tied input, and `$2` for the pure input.

The clobber literal is a comma-separated list of target registers and the portable names `memory`, `cc`, `flags`, `dirflag`, and `fpsr`. `abi:C` and `abi:system` request the selected target's exact caller-saved register set when that ABI exists. Unknown registers, clobbers, and ABIs are rejected before MIR lowering.

The option value is a literal bit set:

| Bit | Contract |
| --- | --- |
| `1` | Pure assembly with no side effects outside its typed result |
| `2` | The assembly requires an aligned stack |
| `4` | Use the Intel assembly dialect on targets that support it |
| `8` | Assembly may unwind |

Unknown option bits are rejected. Pure assembly cannot declare a memory clobber. Assembly cannot unwind through Bray frames, so a set unwind bit is rejected deterministically. Local assembler labels remain within the template. A `label` operand is available only through `branching_assembly` and carries an alternate external transfer target.

The target profile owns inline-assembly availability, target features, register classes, physical registers, clobber ABIs, and supported syntax dialects. The default dialect is the selected target's LLVM syntax. The Intel option is accepted only for an x86 profile that explicitly supports it. LLVM's integrated assembler consumes the validated inline assembly while object generation and linking remain ordinary Bray artifact orchestration. Bray Tack does not select, chain, or configure multiple external assembler programs for one target.

The LLVM backend receives the checked typed descriptors, template, target constraints, expanded ABI clobbers, side-effect flag, stack contract, dialect, and control-flow shape without weakening them. Code generation derives its LLVM constraints and argument/result shapes from those descriptors. Any mismatch with the already checked contract is a compiler invariant failure.

## Related chapters

- [Targets, layout, ABI, and raw memory index](../targets-layout-abi-and-raw-memory.md)
- Previous: [ABI-oriented memory helpers](abi-oriented-memory-helpers.md)
- Next: [Summary](summary.md)
