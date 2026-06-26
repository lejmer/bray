# Axiom 11: Trusted memory power is explicit and bounded

Unchecked memory power is a trusted capability model. Trusted power is named, scoped, declared, and auditable before the function body is read.

A module may opt in to trusted declarations through a code directive. The directive grants permission to declare trusted functions inside that module. Ordinary functions in
the same module remain ordinary functions.

Trusted capabilities are declared at the function level. A trusted function may use exactly the trusted capabilities it declares. If a trusted function declares a
capability it does not use, the compiler should treat that as an error.

The set of trusted capabilities is closed:

- `raw_memory`
- `unchecked_alias`
- `unchecked_init`
- `foreign_call`
- `layout_reinterpret`
- `manual_alloc`
- `device_memory`
- `intrinsic`

Trustedness is local. Calling a trusted function uses that function's declared contract, but does not grant unchecked capabilities to the caller.

A public trusted implementation exposes one of two contracts:

- A safe public wrapper that does not expose trusted capabilities to ordinary callers.
- An explicitly trusted public contract whose callers must opt in to the stated trusted contract.
