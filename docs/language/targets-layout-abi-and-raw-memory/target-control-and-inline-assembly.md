# Target control and inline assembly

This chapter defines Bray operations whose meaning depends on selected-target instructions or optimizer barriers. They are compiler-provided declarations rather than a second command language. Every operation is checked before MIR lowering and remains explicit in MIR and compiled package interfaces.

## Volatile access

`core.target.volatile_load` and `core.target.volatile_store` perform one host-address-space volatile access. Their raw-memory predicates establish validity, alignment, and initialization. A volatile read of a copyable value preserves initialization. A volatile read of a non-copyable value moves the value and consumes the source initialization fact. A volatile store establishes initialization only after the store completes.

`core.target.device_volatile_load` and `core.target.device_volatile_store` select device-memory semantics. They require both `raw_memory` and `device_memory`. A target without a device address space rejects them before code generation.

Volatile access is not atomic access. It creates no synchronization edge and supplies no inter-thread ordering guarantee. Atomic storage and hardware fences use the atomic operation family.

## Pointer addresses

`core.target.expose_address` converts a raw pointer to a target-width unsigned address and explicitly discards provenance. `core.target.from_exposed_address` reconstructs a raw pointer from such an address. Reconstruction does not establish validity, alignment, initialization, alias authority, allocation ownership, or a borrow dependency.

`core.target.address_equal` and `core.target.address_less` compare exposed target addresses. Equality and unsigned ordering are therefore address comparisons rather than provenance or allocation-identity comparisons. The operations are available only where target-width raw addresses are available.

## Barriers and termination

`core.target.compiler_fence` prevents the optimizer from moving memory effects across the fence. It does not emit a hardware synchronization instruction and creates no synchronization edge.

`core.target.abort` performs catastrophic termination without source cleanup, panic propagation, or cancellation propagation. `core.target.debugger_trap` traps and may continue when a debugger resumes execution. `core.target.unreachable` terminates a trusted path whose reachability contract has been violated. MIR marks abort, unreachable, and diverging assembly as non-continuing control flow.

`core.target.spin_loop_hint` emits the selected architecture's non-synchronizing spin hint. It does not yield a Bray task and does not observe cancellation.

## Target facts and feature gates

The selected target profile determines its instruction set, guaranteed instruction features, register classes, physical registers, supported clobber ABIs, and inline-assembly availability. WebAssembly profiles reject inline assembly. `core.target.feature_enabled` accepts a string literal and returns whether that feature is guaranteed by the selected profile.

Assembly feature lists are comma-separated literal feature names. Every named feature must be guaranteed by the selected profile. Unsupported or dynamically computed feature requirements are rejected before MIR lowering.

## Trusted inline assembly

`core.target.assembly<Input, Output>` and `core.target.diverging_assembly<Input>` are trusted compiler-provided declarations. Their template, constraint, clobber, feature, and option operands must be literals. The input is evaluated and moved according to its ordinary typed argument contract. A continuing assembly operation initializes its typed output. A diverging operation has no output and no normal successor.

The declarations require `intrinsic`, `raw_memory`, `device_memory`, `unchecked_alias`, and `unchecked_init`. This capability set makes potential register, memory, device, alias, and initialization effects visible in the containing trusted declaration. Ordinary source cannot invoke the operations without acknowledging those capabilities.

The constraint literal is a comma-separated list. A late output begins with `=`. An early output begins with `=&`. An input-output operand begins with `+`, with `+&` selecting an early input-output operand. Register classes, explicit physical registers, immediate `i`, symbol `s`, and memory `m` constraints are target checked. Continuing assembly has exactly one output and one input contribution. Diverging assembly has exactly one input and no output.

The clobber literal is a comma-separated list of target registers and the portable names `memory`, `cc`, `flags`, `dirflag`, and `fpsr`. `abi:C` and `abi:system` request the selected target's exact caller-saved register set when that ABI exists. Unknown registers, clobbers, and ABIs are rejected before MIR lowering.

The option value is a literal bit set:

| Bit | Contract |
| --- | --- |
| `1` | Pure assembly with no side effects outside its typed result |
| `2` | The assembly requires an aligned stack |
| `4` | Use the Intel assembly dialect on targets that support it |
| `8` | Assembly may unwind |

Unknown option bits are rejected. Pure assembly cannot declare a memory clobber. Assembly cannot unwind through Bray frames, so a set unwind bit is rejected deterministically. Local assembler labels remain within the template. Operand labels and alternate exits are rejected because this declaration family has either one normal successor or no normal successor.

The LLVM backend receives the checked template, target constraints, expanded ABI clobbers, side-effect flag, stack contract, dialect, and divergence fact without weakening them. Code generation treats any mismatch with the already checked contract as a compiler invariant failure.

## Related chapters

- [Targets, layout, ABI, and raw memory index](../targets-layout-abi-and-raw-memory.md)
- Previous: [ABI-oriented memory helpers](abi-oriented-memory-helpers.md)
- Next: [Summary](summary.md)
