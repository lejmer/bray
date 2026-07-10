# Re-exports

An export declaration re-exports a visible declaration path from the current module.

```bray
export impl.Buffer;
export impl.parse_header;
```

Grammar shape:

```text
export declaration-path ';'
```

The exported declaration is made reachable through the current module using its original final name.

The exported declaration path must resolve from the exporting module.

The exported declaration must be visible to the exporting module.

An export declaration can appear only in a module body.

The exported final name occupies the ordinary lookup namespace of the module's exported lookup surface and must not conflict with
another declaration or export in that surface.

An export does not create a new declaration identity.

An export does not rename the declaration.

An export does not execute code, initialize a module, activate implementations, or change overload participation.

An export does not introduce the declaration as an unqualified name inside the exporting module.

Exporting a named implementation makes that implementation declaration reachable through the exported path.

It does not make the implementation participate in another coherence domain unless that coherence domain explicitly makes the implementation visible according to implementation coherence rules.

If a different public name or different public contract is needed, source code declares an explicit wrapper.

## Navigation

- [Language index](../index.md)
- [Modules and packages index](../modules-and-packages.md)
- Previous: [Using declarations](using-declarations.md)
- Next: [Internal re-exports](internal-re-exports.md)
