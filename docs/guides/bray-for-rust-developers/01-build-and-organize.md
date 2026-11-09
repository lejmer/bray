# Bray for Rust developers, part 1: Build and organize an application

Bray is a programming language with ownership and borrowing, explicit resource management, and a compiler that checks
how values are used. Its name comes from the sound a donkey makes. Like its namesake, Bray is stubborn: when the code
stops making sense, it would rather bray than budge.

In this guide, we will build a small pantry inventory, split it into modules, and move the inventory into a library package.
The program adds stock, looks up an item, and prints whether its quantity is low.

We assume you know your way around Rust, including things like functions, structs, optional values, errors as values,
and ownership and borrowing. You don't need to know any Bray yet. We'll start with what you already know and take a
closer look wherever Bray does things differently.

You need an installed Bray toolchain with `bray` on your path and support for your native target. The examples use a
shell and a synchronous executable. Run commands from the project directory unless a step says otherwise.

## Create the project

In Rust, you might start with `cargo new pantry`. In Bray, create the workspace and its root executable package with:

```sh
bray init pantry --package pantry
cd pantry
```

You now have `bray-workspace.json`, `bray-package.json`, and `src/main.bray`. The workspace manifest lists your
packages and targets. The package manifest describes where its source lives and which products to build. A product can
be an executable, a library, or a test program.

Run the generated application:

```sh
bray run
```

It exits without printing anything. The generated `main` is empty, so the donkey has nothing to say yet.

## Add an inventory item

In Rust, a mutable binding lets you change a field through a mutable borrow:

```rust
struct Item {
    name: String,
    quantity: i32,
}

fn add(item: &mut Item, amount: i32) {
    item.quantity += amount;
}
```

Bray uses the same shared and mutable borrow spelling, but field mutability is also declared on the field. Replace
`src/main.bray` with:

```bray
module app;

using std.io;

struct Item
{
    name: string;
    mut quantity: i32;
}

func add(pos item: &mut Item, amount: i32)
{
    item.quantity += amount;
}

func main()
{
    let mut oats: Item =
    {
        name = "oats",
        quantity = 1,
    };

    add(&mut oats, amount = 2);

    if oats.quantity == 3
    {
        std.io.print_line("Stock is ready");
    }
}
```

Run the application:

```sh
bray run
```

The application prints:

```text
Stock is ready
```

You may also see build progress. We started with one unit of oats and added two, so the quantity is three. Our pantry
is off to a modest start.

As in Rust, `let mut oats` permits mutation through the binding, and `&mut oats` gives `add` exclusive mutable access.
Bray also requires `mut quantity` on the field. The immutable `name` field cannot be replaced after initialization,
even through a mutable `Item`.

Bray uses `func` for functions. Parameters are named at the call site by default. The `pos` on `item` permits a
positional argument, so the call is `add(&mut oats, amount = 2)`. A `pos` parameter can also be passed by name. A
function's positional parameters must come before its named-only parameters.

The type of `oats` supplies the type for the construction expression. Each field initializer uses `=` to give the field
its value. The examples follow Bray's standard source style: a multiline construction puts the opening brace on the next
line and includes a trailing comma after the final field. Neither choice is required by the grammar.

## Handle absence and errors

In Rust, a lookup might return `Option<&Item>`, and a fallible print operation might return a `Result`. Bray separates
these two cases too, but their propagation operators differ.

Our first version ignores the result of `print_line`, so we won't find out if printing fails. Let's return the error to
the caller instead.

Replace the `main` function in `src/main.bray` with:

```bray
func main() -> Result<unit, std.io.IoError>
{
    let mut oats: Item =
    {
        name = "oats",
        quantity = 1,
    };

    add(&mut oats, amount = 2);

    if let ?item = find(&oats, name = "oats")
    {
        if low(item)
        {
            try std.io.print_line("Low stock");
        }
        else
        {
            try std.io.print_line("Stock is ready");
        }
    }

    return Ok(unit);
}
```

Add these functions after `add`:

```bray
func find(pos item: &Item, name: string) -> (&Item)?
{
    if item.name == name
    {
        return item;
    }

    return none;
}

func low(pos item: &Item) -> bool
{
    return item.quantity < 3;
}
```

Run `bray run` again. The application still prints `Stock is ready`.

The shared borrow passed to `find` lets it inspect the item without taking ownership.
`find` returns `(&Item)?`: a nullable borrow. The parentheses matter here. The borrow is the value that may be absent.
Returning `item` supplies a present value; `none` supplies an absent value. The `if let ?item` condition binds the
borrow only when the lookup succeeds. That borrow cannot outlive the item it refers to.

`std.io.print_line` returns `Result<unit, std.io.IoError>`. The prefix `try` continues on success and propagates an
error from `main`. `Ok(unit)` reports successful completion. Bray's postfix `?` propagates *absence*; it is not the
operator used for a `Result` error. We will return to both forms in part 2.

Change the quantity passed to `add` from `2` to `1` and run the program again. It prints `Low stock`. Restore `2`
before continuing.

## Split the application into modules

A Rust module can separate inventory code from `main`. Bray also has modules, but a `using` declaration records a
module dependency; it does not bring the module's declarations into unqualified scope.

Create `src/inventory.bray` with the declarations from `src/main.bray`, under a new module declaration:

```bray
module inventory;

struct Item
{
    name: string;
    mut quantity: i32;
}

func add(pos item: &mut Item, amount: i32)
{
    item.quantity += amount;
}

func find(pos item: &Item, name: string) -> (&Item)?
{
    if item.name == name
    {
        return item;
    }

    return none;
}

func low(pos item: &Item) -> bool
{
    return item.quantity < 3;
}
```

Replace `src/main.bray` with:

```bray
module app;

using std.io;
using inventory;

func main() -> Result<unit, std.io.IoError>
{
    let mut oats: inventory.Item =
    {
        name = "oats",
        quantity = 1,
    };

    inventory.add(&mut oats, amount = 2);

    if let ?item = inventory.find(&oats, name = "oats")
    {
        if inventory.low(item)
        {
            try std.io.print_line("Low stock");
        }
        else
        {
            try std.io.print_line("Stock is ready");
        }
    }

    return Ok(unit);
}
```

Run `bray run`. Our oats are still ready, and the output is still `Stock is ready`.

`using inventory;` declares the dependency on the other logical module. We still write `inventory.Item` and
`inventory.add`. Bray wants both the dependency and the qualified path.

Bray declarations are public by default where visibility applies. `Item`, `add`, `find`, and `low` need no `pub`
spelling. For now, both modules belong to the same executable package, so we haven't marked any of them `internal`.

## Move the inventory into a library package

In Rust, moving code into another crate changes both the Cargo dependency and the path used to reach the code. Bray
likewise separates the package dependency in a manifest from the module dependency in source.

Create the directories `stock/src` and `checks` under the workspace root. Move `src/inventory.bray` to
`stock/src/inventory.bray`. Its `module inventory;` declaration stays the same: a source module path is relative to its
package and does not include the package identity.

We are turning the inventory into a library used by another package. Its `low` function is an implementation detail, so add
`internal` to its declaration in `stock/src/inventory.bray`:

```bray
internal func low(pos item: &Item) -> bool
{
    return item.quantity < 3;
}
```

In `bray-workspace.json`, replace the `packages` property with the following. Leave the generated target and other fields
unchanged, so `native` still names your host target.

```json
"packages": [
  {
    "path": ".",
    "role": "root",
    "features": []
  },
  {
    "path": "stock",
    "role": "vendored",
    "features": []
  }
]
```

Create `stock/bray-package.json`:

```json
{
  "format": 1,
  "identity": "stock",
  "version": {
    "workspace": true
  },
  "features": [],
  "source_roots": [
    {
      "name": "main",
      "path": "src"
    }
  ],
  "products": [
    {
      "name": "inventory",
      "kind": "library",
      "source_roots": [
        "main"
      ],
      "targets": [
        "native"
      ],
      "dependencies": [],
      "outputs": [
        "package_interface",
        "package_implementation",
        "package_native_implementation",
        "static_library"
      ]
    }
  ]
}
```

The workspace owns both package directories. The `vendored` role identifies `stock` as a workspace dependency. Bray
doesn't fetch it for us; we just created it. Its package identity is `stock`, and its library product is `inventory`.
This library has no `main`. The `main` function remains in the separate `pantry` executable.

Replace the root `bray-package.json` with the following. Its `application` product is still an executable, and it gives
that executable an explicit dependency on the `stock` library. It also adds a test product for the next step:

```json
{
  "format": 1,
  "identity": "pantry",
  "version": {
    "workspace": true
  },
  "features": [],
  "source_roots": [
    {
      "name": "main",
      "path": "src"
    },
    {
      "name": "checks",
      "path": "checks"
    }
  ],
  "products": [
    {
      "name": "application",
      "kind": "executable",
      "source_roots": [
        "main"
      ],
      "targets": [
        "native"
      ],
      "dependencies": [
        {
          "package": "stock",
          "product": "inventory"
        }
      ],
      "outputs": [
        "executable"
      ]
    },
    {
      "name": "checks",
      "kind": "test",
      "source_roots": [
        "checks"
      ],
      "targets": [
        "native"
      ],
      "dependencies": [
        {
          "package": "stock",
          "product": "inventory"
        }
      ],
      "outputs": [
        "executable"
      ]
    }
  ]
}
```

Replace `src/main.bray` with the following. The paths now start with the `stock` package identity, and the
`using internal` declaration acknowledges the call to `low`:

```bray
module app;

using std.io;
using stock.inventory;
using internal stock.inventory.low;

func main() -> Result<unit, std.io.IoError>
{
    let mut oats: stock.inventory.Item =
    {
        name = "oats",
        quantity = 1,
    };

    stock.inventory.add(&mut oats, amount = 2);

    if let ?item = stock.inventory.find(&oats, name = "oats")
    {
        if stock.inventory.low(item)
        {
            try std.io.print_line("Low stock");
        }
        else
        {
            try std.io.print_line("Stock is ready");
        }
    }

    return Ok(unit);
}
```

Run `bray run --product application`. It prints `Stock is ready` again. The manifest makes the library product available,
and `using stock.inventory;` records the source module dependency.

The call to `low` now crosses a package boundary. Without `using internal stock.inventory.low;`, the compiler rejects
that access. The acknowledgment applies to that path, so the call itself can use `stock.inventory.low(item)` without
another `internal` keyword. Bray also supports a local acknowledgment at the use site.

`internal` doesn't make a declaration private. A consuming package can use it by acknowledging the internal path.
That acknowledgment makes the dependency visible, but it doesn't turn an internal detail into a stable public API.
Acknowledging one path doesn't grant access to unrelated internal declarations.

## Add a test

Rust's `#[test]` marks a function for the test runner. Bray uses `@test` and a test product. The `checks` product in
the package manifest selects the `checks` source root and depends on the same library product as the application.

Create `checks/inventory.bray`:

```bray
@test
module inventory_checks;

using stock.inventory;

@test
func adds_stock()
{
    let mut oats: stock.inventory.Item =
    {
        name = "oats",
        quantity = 1,
    };

    stock.inventory.add(&mut oats, amount = 2);

    assert(oats.quantity == 3);
}
```

Run the test product:

```sh
bray test --product checks
```

The runner reports that `adds_stock` passed. The assertion checks the result through the same library dependency the
application uses. The module-level `@test` makes this contribution available to test products.

For an extra check, change the amount in the test to `1` while leaving the expected quantity at `3`. Run the test again:
the assertion fails. Restore the amount to `2` and confirm that the test passes.

## Continue

Part 2 will explore unions, patterns, and error handling in more detail. Until then, the language reference covers
[modules and packages](../../language/modules-and-packages.md), [callables](../../language/callables.md),
[ownership and borrowing](../../language/ownership-and-borrowing.md), and
[nullable expressions](../../language/expressions/nullable-and-absence-expressions.md). The
[Bray source style guide](../../contributing/bray-style-guide.md) describes the layout used by the examples.
