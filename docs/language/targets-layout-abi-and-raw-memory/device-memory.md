# Device memory

Device memory support is target-conditional and lives under `std.memory.device`.

When a target profile exposes device memory conditions, the standard library can provide recognized device memory
declarations.

Device memory declarations are ordinary standard-library declarations whose contracts name the device, address space,
access mode, byte count, alignment, synchronization state, and transfer behavior involved.

The required device-memory declaration families are:

- device allocation owners,
- device buffer owners,
- host-to-device transfer operations,
- device-to-host transfer operations,
- device synchronization operations,
- scoped host mappings for device memory that can be mapped into host-accessible storage.

Device memory operations use the `device_memory` trusted capability.

Device memory operations that call foreign platform APIs also use `foreign_call`.

Device memory operations that read or write host raw memory also use the required `raw_memory`, `unchecked_init`, or
`unchecked_alias` capabilities for that host access.

A device allocation owner is linear and carries the ownership and release obligation for one device allocation.

A device buffer owner is linear and carries the ownership, element type, capacity, initialized-device-state, and release
obligation for one contiguous device allocation.

Device memory is not host-accessible raw memory unless a recognized mapping declaration creates a scoped host mapping.

A scoped host mapping produces the raw pointer validity, alignment, initialization, synchronization, and access
conditions declared by the mapping contract.

Those conditions remain valid only for the mapping's scoped capability lifetime.

Leaving the mapping scope releases the scoped capability, performs the mapping's required synchronization, and
invalidates raw pointer conditions that depend on the mapping.

Transfers between host memory and device memory must state which host conditions they require, which device conditions
they require, which conditions they establish, and which conditions they invalidate.

Device transfers that can overlap with task or thread execution participate in the
[cross-run memory rules](../async-and-concurrency/cross-run-memory-model.md).

## Navigation

- [Language index](../index.md)
- [Targets, layout, ABI, and raw memory index](../targets-layout-abi-and-raw-memory.md)
- Previous: [Raw allocation and buffers](raw-allocation-and-buffers.md)
- Next: [ABI-oriented memory helpers](abi-oriented-memory-helpers.md)
