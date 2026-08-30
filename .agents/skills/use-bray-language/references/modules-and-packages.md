# Modules and Packages

**Specification:** [Modules and packages](https://github.com/lejmer/bray/blob/develop/docs/language/modules-and-packages.md)

## Contents

- [Packages, products, and source graphs](#packages-products-and-source-graphs)
- [Module declarations and split modules](#module-declarations-and-split-modules)
- [Using declarations and re-exports](#using-declarations-and-re-exports)
- [Internal and trusted modules](#internal-and-trusted-modules)
- [Conditional module contributions](#conditional-module-contributions)
- [Executable and test entries](#executable-and-test-entries)
- [Choose the boundary by intent](#choose-the-boundary-by-intent)

## Packages, products, and source graphs

**Core model:** The package and build layer supplies package identity, version, products, target constraints, source graphs, and dependency graphs. Every selected Bray source unit explicitly contributes declarations to a module inside that package, while module paths, `using`, exports, and visibility determine how those declarations are reached.

A package manifest can select library, executable, and test products from different source roots:

```json
{
  "format": 1,
  "identity": "example.data",
  "version": "1.4.0",
  "features": [],
  "source_roots": [
    { "name": "library", "path": "src" },
    { "name": "application", "path": "app" },
    { "name": "tests", "path": "tests" }
  ],
  "products": [
    {
      "name": "library",
      "kind": "library",
      "source_roots": ["library"],
      "targets": ["native"],
      "outputs": ["package_interface", "package_implementation", "static_library"]
    },
    {
      "name": "tool",
      "kind": "executable",
      "source_roots": ["library", "application"],
      "targets": ["native"],
      "outputs": ["executable"]
    },
    {
      "name": "integration",
      "kind": "test",
      "tested_library": "library",
      "source_roots": ["tests"],
      "targets": ["native"],
      "outputs": ["executable"]
    }
  ]
}
```

[Package identity and version](https://github.com/lejmer/bray/blob/develop/docs/language/modules-and-packages/module-paths-and-package-identity.md) stay outside Bray source. A package can provide its Semantic Versioning value directly, as above, or explicitly inherit the workspace package version. Versions do not appear in source paths.

Each [product](https://github.com/lejmer/bray/blob/develop/docs/language/modules-and-packages/package-products-and-source-graphs.md) selects one explicit source graph and dependency graph. A product is a compilation surface, not a module or lookup scope. Library products expose their reachable public declaration graph. Executable products form one entry point. Test products form independent test entries and can consume a tested library through its compiled public contract.

Each activation of an executable, test, or loadable library is a distinct product instance and owns its realized product-static storage. A library compilation exposes no runtime entry point.

A source-declared path is relative to the current package. If this package declares `module codec;`, another package reaches that module as `example.data.codec`. Package dependencies are selected by the build layer, while source uses their visible package paths.

## Module declarations and split modules

A source unit can use one semicolon-form module declaration followed by unbraced items and then braced module contributions. It can
also contain only braced module contributions. After the first braced contribution, every remaining top-level contribution stays
braced.

The thin root `src/codec.bray` declares the logical module:

```bray
/// Encodes and decodes packets.
module codec;
```

The concept file `src/codec/packet.bray` contributes declarations to the same module:

```bray
module codec;

struct Packet
{
    header: Header;
    payload: Bytes;
}
```

The independent concept file `src/codec/decoding.bray` repeats the explicit module identity and its own `using` declarations:

```bray
module codec;

using std.bytes;

func decode(pos source: &[u8]) -> Result<Packet, DecodeError>
{
    return decode_packet(source);
}

@test
module codec.tests
{
    using codec;

    @test
    func decodes_empty_packet() -> Result<unit, DecodeError>
    {
        let _: codec.Packet = try codec.decode(empty_packet_bytes());

        return Ok(unit);
    }
}
```

The unbraced declarations contribute to `codec`. The later block contributes independently to `codec.tests`, so its `@test` gate
does not gate the production prefix. A block suffix names complete package-level module paths and does not inherit the source-unit
path, directives, visibility, or trusted state.

[Split module declarations](https://github.com/lejmer/bray/blob/develop/docs/language/modules-and-packages/split-modules.md) merge into one logical declaration scope. File paths organize source but do not name modules, and source enumeration order does not affect lookup. Every source unit remains understandable on its own.

Braced declarations let one source unit contribute to several package-level modules:

```bray
module codec.model
{
    struct Metadata
    {
        version: u32;
    }
}

module codec.validation
{
    using codec.model;

    func supported(pos metadata: &codec.model.Metadata) -> bool
    {
        return metadata.version == 1;
    }
}
```

Each braced declaration names its complete module path. The blocks are package-level contributions rather than lexically nested modules.

## Using declarations and re-exports

[`using`](https://github.com/lejmer/bray/blob/develop/docs/language/modules-and-packages/using-declarations.md) records that the current module intentionally depends on a visible package, module, or declaration path. References remain qualified because `using` participates in checking and tooling without changing unqualified name lookup.

```bray
module api;

using codec;
using internal codec.impl.Decoder;

export codec.Packet;
export codec.impl.Decoder;

func decode(pos source: &[u8]) -> Result<codec.Packet, DecodeError>
{
    return codec.decode(source);
}
```

An [`export`](https://github.com/lejmer/bray/blob/develop/docs/language/modules-and-packages/re-exports.md) makes the selected declaration reachable through the exporting module under its existing final name and identity. It does not rename the declaration or introduce an unqualified name inside `api`.

The `using internal` declaration acknowledges access to `codec.impl.Decoder`. Its [re-export remains internal](https://github.com/lejmer/bray/blob/develop/docs/language/modules-and-packages/internal-re-exports.md). A public boundary over internal representation uses a new wrapper whose public contract does not expose that internal declaration:

```bray
module api;

using internal codec.impl.Buffer;

struct Buffer
{
    internal storage: codec.impl.Buffer;
}
```

## Internal and trusted modules

Module visibility applies to the path. Public is the default and is normally omitted. An internal module or declaration reached from outside its intended scope requires explicit internal-use acknowledgement.

```bray
internal module codec.impl;

internal struct Decoder
{
    state: DecoderState;
}
```

A [`trusted` module](https://github.com/lejmer/bray/blob/develop/docs/language/modules-and-packages/trusted-modules.md) admits trusted declarations. Each trusted declaration still states its own trusted role:

```bray
trusted internal module codec.platform;

trusted internal func decode_valid_packet(pos source: &[u8]) -> Packet
{
    return trusted platform_decode_packet(source);
}
```

All split contributions to one logical module agree on public or internal visibility and on trusted-module state. `trusted` precedes `internal` in a module header.

Module bodies contain declarations and have no runtime initialization phase. Runtime work belongs in functions, constructors, lifecycle declarations, tasks, tests, and other executable bodies.

## Conditional module contributions

[`@target(...)`](https://github.com/lejmer/bray/blob/develop/docs/language/modules-and-packages/target-constraints-and-gates.md) selects a complete module contribution using a compile-time target expression. These mutually exclusive blocks provide one logical module on targets with and without 64-bit atomic storage:

```bray
@target(target.atomic.U64)
module counters
{
    using std.atomic;

    struct Counter
    {
        value: std.atomic.Atomic<u64>;
    }

    func counter() -> Counter
    {
        return
        {
            value = std.atomic.Atomic<u64>(0)
        };
    }

    func value(pos counter: &Counter) -> u64
    {
        return counter.value.load(order = std.atomic.LoadOrder.Relaxed);
    }
}

@target(!target.atomic.U64)
module counters
{
    struct Counter
    {
        mut value: u64;
    }

    func counter() -> Counter
    {
        return
        {
            value = 0
        };
    }

    func value(pos counter: &Counter) -> u64
    {
        return counter.value;
    }
}
```

The condition is a compile-time boolean over compiler-known target properties and language-defined constants. It is evaluated before declarations from the selected source graph are available.

[`@test`](https://github.com/lejmer/bray/blob/develop/docs/language/modules-and-packages/conditional-module-contributions.md) enables a complete module contribution only for test products. When both directives apply, every gate must enable the contribution:

```bray
@test
@target(target.atomic.U64)
module counters.tests
{
    using counters;

    @test
    func starts_at_zero()
    {
        let counter: counters.Counter = counters.counter();

        assert(counters.value(&counter) == 0);
    }
}
```

A disabled contribution remains lexically and syntactically valid, but it does not participate in semantic checking, module merging, lookup, coherence, entry formation, or the compiled product surface. Contribution gates select source declarations without changing module identity or runtime behavior.

## Executable and test entries

An executable product resolves exactly one entry point. [`@entrypoint`](https://github.com/lejmer/bray/blob/develop/docs/language/modules-and-packages/library-and-executable-products.md) can identify any conforming module-level function explicitly:

```bray
module application;

using codec;

@entrypoint
async func launch() -> Result<unit, StartupError>
{
    let packet: codec.Packet = try await load_packet();

    return await process_packet(packet);
}
```

Without an explicit directive, an executable resolves exactly one conforming module-level `main`. Entry points take no caller-supplied parameters or generic parameters and return `unit`, `Result<unit, E>`, or `i32`. An async entry point becomes the product's root structured task scope.

```bray
module application;

func main() -> i32
{
    return 0;
}
```

A [test-only module contribution](https://github.com/lejmer/bray/blob/develop/docs/language/modules-and-packages/test-products-and-entries.md) can contain ordinary helpers, parallel tests, command-wide serial tests, and async tests:

```bray
@test
module integration
{
    using example.data.codec;

    func sample_packet() -> &[u8]
    {
        return packet_fixture();
    }

    @test
    func decodes_sample() -> Result<unit, DecodeError>
    {
        let _: example.data.codec.Packet = try example.data.codec.decode(sample_packet());

        return Ok(unit);
    }

    @test(serial)
    func updates_process_configuration()
    {
        update_process_configuration();
    }

    @test
    async func decodes_stream() -> Result<unit, DecodeError>
    {
        let source: &[u8] = try await read_packet_fixture();
        let _: example.data.codec.Packet = try example.data.codec.decode(source);

        return Ok(unit);
    }
}
```

Only functions carrying `@test` form entries. Bare `@test` entries may run in parallel, while `@test(serial)` excludes overlap with every test selected by that command. A test entry has no receiver, generic parameters, or caller-supplied parameters and returns `unit` or `Result<unit, E>`.

## Choose the boundary by intent

| Intent                                                | Surface                                                                                                                                                     |
|-------------------------------------------------------|-------------------------------------------------------------------------------------------------------------------------------------------------------------|
| Define distribution and dependency identity           | [Package manifest](https://github.com/lejmer/bray/blob/develop/docs/language/modules-and-packages/package-versions.md)                                      |
| Define a declaration namespace                        | [`module path;` or `module path { ... }`](https://github.com/lejmer/bray/blob/develop/docs/language/modules-and-packages/module-declarations.md)            |
| Organize one module across files                      | [Repeated split module declarations](https://github.com/lejmer/bray/blob/develop/docs/language/modules-and-packages/split-modules.md)                       |
| Record an intentional dependency                      | [`using path;`](https://github.com/lejmer/bray/blob/develop/docs/language/modules-and-packages/using-declarations.md)                                       |
| Expose an existing declaration through another module | [`export path;`](https://github.com/lejmer/bray/blob/develop/docs/language/modules-and-packages/re-exports.md)                                              |
| Acknowledge internal access                           | [`using internal path;` or local `internal` access](https://github.com/lejmer/bray/blob/develop/docs/language/modules-and-packages/module-visibility.md)    |
| Select source by product kind                         | [`@test` on a module](https://github.com/lejmer/bray/blob/develop/docs/language/modules-and-packages/conditional-module-contributions.md)                   |
| Select source by target                               | [`@target(condition)` on a module](https://github.com/lejmer/bray/blob/develop/docs/language/modules-and-packages/target-constraints-and-gates.md)          |
| Form a program root                                   | [`@entrypoint` or one conforming `main`](https://github.com/lejmer/bray/blob/develop/docs/language/modules-and-packages/library-and-executable-products.md) |
| Form a test entry                                     | [`@test` or `@test(serial)` on a function](https://github.com/lejmer/bray/blob/develop/docs/language/modules-and-packages/test-products-and-entries.md)     |

**Remember:** The build layer selects package and product graphs, source declares explicit logical modules, `using` records qualified dependencies, exports preserve identity, and directives select whole contributions or product entries without changing ordinary module semantics.
