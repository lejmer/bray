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

The exported declaration path must resolve from the exporting module. A cross-module export path requires a matching
`using` in that module. The export does not itself declare that dependency.

A consumer declares its own dependency on the re-exporting namespace path it names. The exporting module's dependency
declarations do not become the consumer's declarations.

The exported declaration must be visible to the exporting module.

An export declaration can appear only in a module body.

The exported final name occupies the ordinary lookup namespace of the module's exported lookup surface and must not
conflict with another declaration or export in that surface.

An export does not create a new declaration identity.

Re-exporting a static preserves its open instance template and canonical static instance identity. It does not
materialize storage, duplicate an instance, change the owning product or attachment, or add an initialization event.

An export does not rename the declaration.

An export does not execute code, initialize a module, activate implementations, or change overload participation.

An export does not introduce the declaration as an unqualified name inside the exporting module.

Exporting a named implementation makes that implementation declaration reachable through the exported path.

It does not make the implementation participate in another coherence domain unless that coherence domain explicitly
makes the implementation visible according to implementation coherence rules.

If a different public name or different public contract is needed, source code declares an explicit wrapper.

## Navigation

- [Language index](../index.md)
- [Modules and packages index](../modules-and-packages.md)
- Previous: [Using declarations](using-declarations.md)
- Next: [Internal re-exports](internal-re-exports.md)
