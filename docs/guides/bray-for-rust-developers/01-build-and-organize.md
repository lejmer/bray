# Bray for Rust developers, part 1: Build and organize an application

Bray is a programming language with ownership, borrowing, explicit resource management, and a compiler that checks how values are used. If that sounds familiar, good. Rust has already prepared you for a programming language that occasionally knows better than you do.

Bray takes its name from the sound a donkey makes. Like its namesake, it is stubborn, opinionated, and thoroughly unimpressed by your intentions. If your code doesn't make sense, Bray would rather complain than politely pretend otherwise.

In this guide, we'll build a small pantry inventory, split it into modules, and eventually move the inventory into its own library package. Our application will add stock, look up an item, and report whether its quantity is low. Nothing revolutionary, but it does give us an excuse to explore the language without immediately implementing a distributed database.

We assume you're familiar with Rust, including functions, structs, optional values, errors as values, ownership, and borrowing. You don't need to know any Bray yet. We'll start with familiar concepts and focus on the places where Bray has decided that Rust's approach wasn't quite opinionated enough.

You'll need an installed Bray toolchain with `bray` on your path and support for your native target. The examples use a shell and a synchronous executable. Run commands from the project directory unless instructed otherwise.

## Create the project

In Rust, you'd probably start with `cargo new pantry`. Bray has its own CLI, because apparently the world needed another build tool.

Create the workspace and its root executable package:

```sh
bray init pantry --package pantry
cd pantry
```

You now have `bray-workspace.json`, `bray-package.json`, and `src/main.bray`.

Cargo uses `Cargo.toml` for both package and workspace configuration. Bray separates these concerns into two manifests, giving you twice as many JSON files to admire.

The workspace manifest lists your packages and targets. The package manifest describes where its source lives and which products to build. A product can be an executable, a library, or a test program.

Run the generated application:

```sh
bray run
```

Nothing happens.

Well, technically, the application builds, runs, and exits successfully. It simply produces no output because the generated `main` is empty. The donkey has nothing to complain about yet. Let's fix that.

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

Simple enough. Bray uses the same shared and mutable borrow spelling, but adds field-level mutability declarations.

Now replace `src/main.bray` with:

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

Run it:

```sh
bray run
```

The application prints:

```text
Stock is ready
```

Success. We've added two units of oats to our original one, bringing the total to three. The pantry is operational (investors will be *thrilled*).

### Mutability

As in Rust, `let mut oats` permits mutation through the binding, and `&mut oats` gives `add` exclusive mutable access. But Bray also requires `mut quantity` on the field itself. The `name` field is immutable and cannot be replaced after initialization, even through a mutable `Item`.

In other words, having permission to mutate a value doesn't automatically mean having permission to mutate everything inside it. A distinction that is well worth making.

### Functions and arguments

Bray uses `func` where Rust uses `fn` (not exactly revolutionary, but `fn` is hideous so it *is* an improvement, you'll come around). Bray's more opinionated about function arguments though. They are **named by default**, unlike most other programming languages, where positional is the default. Don't sweat, though, the `pos` modifier on `item` permits a positional argument, which is why we write:

```bray
add(&mut oats, amount = 2);
```

Positional parameters must appear before named-only parameters in a function declaration. This makes call sites more explicit about what arguments mean. No more staring at `configure(true, false, 3)` and wondering which of those booleans ruined your afternoon. Also, we like consistency, so a `pos` parameter can also be passed by name.

### Constructing values

In Rust, we'd construct a struct with something like `Item { ... }`. Bray allows us to omit the type name when the expected type is already known:

```bray
let mut oats: Item =
{
    name = "oats",
    quantity = 1,
};
```

The annotation on `oats` supplies the type. Bray also uses `=` between field names and values, rather than Rust's JSON-like `:` syntax.

| **Note:** The examples follow Bray's standard source style: a multiline construction places its opening brace on the next line and includes a trailing comma after the final field. Neither is a grammar requirement. The donkey has opinions about formatting, but it doesn't *enforce* them.

## Handle absence and errors

Rust developers are familiar with `Option<T>` and `Result<T, E>`. Sometimes a value doesn't exist. Sometimes an operation fails. Sometimes life wasn't complicated enough and both happen in the same function. Bray also distinguishes absence from errors, but uses different propagation operators.

`std.io.print` and `std.io.print_line` return `unit` and panic if output fails, like Rust's `print!` and `println!`.

Replace `main` in `src/main.bray` with:

```bray
func main()
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
            std.io.print_line("Low stock");
        }
        else
        {
            std.io.print_line("Stock is ready");
        }
    }
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

### Nullable values

The shared borrow passed to `find` allows it to inspect the item without taking ownership. Rust would typically express the return type as `Option<&Item>`. Bray uses:

```bray
(&Item)?
```

This represents a value that may contain a borrow or may be absent. The parentheses matter: it's the borrow that may be absent (not some mysterious alternative interpretation of `&Item`). When the item exists, we return it directly:

```bray
return item;
```

Otherwise:

```bray
return none;
```

Notice how there's no `Some(item)`? Well, Bray has decided that if the type already tells you a value may be absent, wrapping every present value in another constructor is unnecessary ceremony.

To inspect the result, we use:

```bray
if let ?item = find(&oats, name = "oats")
```

This behaves much like Rust's `if let Some(item)`, binding `item` only when the value is present. And because the result contains a borrow, that borrow cannot outlive the original item (no dangling references).

### Propagating errors

When you need to handle an output failure, use a writer. The writer operations return `Result` and preserve `IoError`, including transfer progress.

For example, this standalone helper writes and flushes a message:

```bray
using std.io.StandardOutputWriter;
using std.string;

func write_message(pos text: string) -> Result<unit, std.io.IoError>
{
    let mut output: std.io.StandardOutput = std.io.standard_output();

    try output(std.io.Writer).write_all(text.as_bytes());

    return output(std.io.Writer).flush();
}
```

Bray uses the prefix operator `try` to propagate errors. In this helper, `try` continues after a successful write or returns the error to the caller. It does what Rust's postfix `?` does for a `Result`. The final `return` passes the flush result to the caller.

Each `write` and `flush` call acquires its own stream lock. The writer's `write_all` method may call `write` more than once. `print` and `print_line` hold one lock across all writes and the final flush.

Bray has its own postfix `?`, however, that operator propagates **absence**, and not errors. In other words, Rust's `?` and Bray's `?` are not interchangeable. Same symbol, different responsibility. Naturally.

We'll look more closely at both forms in part 2.

### Returning values

Bray's `unit` is the equivalent of Rust's `()`.

A function such as our `main` can omit its result type when it returns `unit`. A fallible function can return `Result<unit, E>`. Its successful `return Ok(unit);` is equivalent to Rust's `Ok(())`.

Unlike Rust, where a function can return its final expression implicitly, Bray requires an explicit `return` along each normal path that returns a non-`unit` value. It's a few more keystrokes, but at least nobody has to debate whether removing a semicolon changed a function's return value.

Before continuing, change the amount passed to `add` from `2` to `1`, then run the application again. It should print:

```text
Low stock
```

Our inventory system now detects a shortage of oats. Remember to restore the amount to `2` before moving on.

## Split the application into modules

We've reached the point where our application has several functions and a struct. Naturally, it's time to organize the code. Rust uses modules to separate declarations, and `use` can bring names into scope. Bray has modules too, but its `using` declaration works differently: it declares a dependency on another module without importing that module's declarations into unqualified scope. That means you still have to write the qualified path.

The compiler would appreciate knowing exactly where your dependencies come from, thank you very much.

Now, create `src/inventory.bray`:

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

And replace the contents of `src/main.bray` again, but this time with:

```bray
module app;

using std.io;
using inventory;

func main()
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
            std.io.print_line("Low stock");
        }
        else
        {
            std.io.print_line("Stock is ready");
        }
    }
}
```

Run:

```sh
bray run
```

The output is still:

```text
Stock is ready
```

The oats have survived their first architectural refactoring!

### Module dependencies

Notice that we declared:

```bray
using inventory;
```

But still call:

```bray
inventory.add(&mut oats, amount = 2);
```

Again, that's intentional. `using` declares the dependency. It doesn't dump the module's contents into the current scope and hope nobody introduces a conflicting name later. Indeed, both the dependency declaration and the qualified reference are required to satisfy the donkey.

### Public by default

Contrary to most other languages, Bray declarations are public by default wherever visibility applies. That means `Item`, `add`, `find`, and `low` don't need a `public` keyword (even though being explicit and adding `public` is technically allowed).

For now, both modules are part of the same executable package, so no additional visibility would make sense anyway.

## Move the inventory into a library package

However, our pantry application is growing up. It's time to give the inventory its own library package, so other applications can enjoy the thrill of counting oats too!

In Rust, moving code into a separate crate involves updating Cargo dependencies and changing the paths used to reference that code. Bray has the same broad distinction: package dependencies belong in manifests, while module dependencies are declared in source code. The good news is that the separation is explicit. The bad news is that we're about to write JSON.

### Create the library

Create the directories `stock/src` and `checks` under the workspace root.

Move:

```text
src/inventory.bray
```

to:

```text
stock/src/inventory.bray
```

Keep the module declaration unchanged:

```bray
module inventory;
```

A source module path is relative to its package and doesn't include the package identity. We're turning the inventory into a library consumed by another package. The `low` function is an implementation detail, so we'll mark it `internal`.

In `stock/src/inventory.bray`, change its declaration to:

```bray
internal func low(pos item: &Item) -> bool
{
    return item.quantity < 3;
}
```

We'll see what `internal` actually means shortly. Spoiler: it's not quite Rust's `pub(crate)`.

### Update the workspace

In `bray-workspace.json`, replace the `packages` property with:

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

Leave the generated target and other fields unchanged, so `native` still refers to your host target.

The workspace now owns two package directories. The `vendored` role identifies `stock` as a workspace dependency.

### Configure the library package

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

The package identity is `stock`, and its library product is named `inventory`. Also, note that the library has no `main` function. That remains in the separate `pantry` executable, where it belongs. The manifest explicitly lists the library's outputs. It does not infer your intentions from the general shape of the directory tree.

### Update the executable package

Replace the root `bray-package.json` with:

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
      "name": "tests",
      "path": "tests"
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
      "name": "tests",
      "kind": "test",
      "source_roots": [
        "tests"
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

Our `application` product remains an executable, but now explicitly depends on the `stock` package's `inventory` library product. We've also introduced a `checks` test product. We'll use that shortly, once we're done configuring our increasingly sophisticated oat-management infrastructure.

### Update the application

Replace `src/main.bray` with:

```bray
module app;

using std.io;
using stock.inventory;
using internal stock.inventory.low;

func main()
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
            std.io.print_line("Low stock");
        }
        else
        {
            std.io.print_line("Stock is ready");
        }
    }
}
```

The module paths now begin with the package identity, `stock`.

Run the executable product:

```sh
bray run --product application
```

Once again:

```text
Stock is ready
```

(If only every software migration were this uneventful.)

### Accessing internal declarations

You may have noticed this new line:

```bray
using internal stock.inventory.low;
```

This is one of Bray's more unusual design choices. We marked `low` as `internal` because it's an implementation detail of the library. But our executable lives in another package and still needs to call it. Rather than making the function entirely inaccessible, Bray allows consumers to explicitly acknowledge that they're depending on an internal declaration. The declaration above acknowledges access to that path for the module, allowing calls such as:

```bray
stock.inventory.low(item)
```

without additional annotation.

Alternatively, we can remove `using internal stock.inventory.low;` while keeping the ordinary `using stock.inventory;`, and acknowledge the call directly:

```bray
if internal stock.inventory.low(item)
{
    std.io.print_line("Low stock");
}
else
{
    std.io.print_line("Stock is ready");
}
```

Here, the local `internal` acknowledges only this particular call. Without either form of acknowledgment, the compiler rejects access.

And yes, in the this case we still need:

```bray
using stock.inventory;
```

The local `internal` prefix doesn't declare the module dependency itself.

### Why not just use something akin to `publ(crate)`?

Rust's `pub(crate)` makes a declaration accessible within its crate but inaccessible to external crates. Bray's `internal` follows a different philosophy.

An internal declaration can be accessed from another package, but only with an explicit acknowledgment of the internal path. This makes the dependency visible without pretending that the declaration is part of a stable public API. Acknowledging one path also doesn't grant access to unrelated internal declarations. Bray allows this because forcing encapsulation completely on programmers is dumb and annoying.

In short, Bray lets you depend on implementation details, but insists that you admit what you're doing. The compiler isn't your manager. It won't forbid every questionable architectural decision. It will, however, make sure you can't pretend the decision happened by accident.

## Add a test

We have an inventory system that adds stock and determines whether it's low. So far, we've verified its behavior by repeatedly running the executable and inspecting the output like responsible programmers from 1978. Let's automate that.

Rust uses `#[test]` to mark test functions. Bray uses `@test` and a dedicated test product. The `checks` product we added to the package manifest selects the `checks` source root and depends on the same inventory library as the application.

Create `tests/inventory.bray`:

```bray
@test
module tests;

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
bray test --product tests
```

The test runner reports that `adds_stock` should pass. The assertion checks the result through the same library dependency used by the application, rather than reaching into the library's implementation. The module-level `@test` makes this module contribution available to test products.

### Make the test fail

A passing test is reassuring, but it's also worth verifying that the test actually detects incorrect behavior.

Change:

```bray
stock.inventory.add(&mut oats, amount = 2);
```

to:

```bray
stock.inventory.add(&mut oats, amount = 1);
```

Keep the expected quantity at `3`, then run:

```sh
bray test --product tests
```

The assertion fails, as it should. Now restore the amount to `2` and run the test again to confirm it passes again.

## Next up

We've created a Bray workspace, built a small executable, used mutable and shared borrows, handled nullable values and errors, organized code into modules, extracted a library package, acknowledged an internal dependency, and written a test.

Part 2 will explore unions, patterns, and error handling in more detail. If you want even drier reading material, the language reference covers [modules and packages](../../language/modules-and-packages.md), [callables](../../language/callables.md), [ownership and borrowing](../../language/ownership-and-borrowing.md), and [nullable expressions](../../language/expressions/nullable-and-absence-expressions.md) in much more detail.

The [Bray source style guide](../../contributing/bray-style-guide.md) describes the layout used throughout these examples.
