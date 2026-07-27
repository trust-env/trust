# Trust

Trust stands for "Trust is Rust",
or "Trust Rust Services Together."
or "Trust Rust to Serve Things."
or "Trust Rust to Serve Trust Rust Services Together."
or anything else you want it to stand for.

A declarative DSL for wiring Rust services together.  
Does it stand for something? Yes. We'll figure out what later.

It's like Terraform, but for your app layer. And without the 40-minute plan.

---

## Documentation

- [Tutorial](doc/tutorial.md) — hands-on walkthrough of the `.tr` DSL
- [Contributing](CONTRIBUTING.md) — running the test suite, adding new language constructs

---

## Why

Wiring Rust microservices by hand is fine. Until it isn't.  
Trust is the thing you write before you regret not writing it.

## Status

Early. Very early. The spec exists. The engine doesn't yet.  
Use at your own risk — which is currently zero, because nothing runs.

*Built in Rust. Named by committee. Shipped eventually.*

---

## Requirements

- [Rust](https://www.rust-lang.org/tools/install) 1.85+ (edition 2024)

---

## Build

```bash
cargo build
```

For a release build:

```bash
cargo build --release
```

---

## Run

Pass one or more `.tr` files as arguments:

```bash
cargo run -- <file.tr> [file2.tr ...]
```

Or with the release binary:

```bash
./target/release/trust <file.tr> [file2.tr ...]
```

The parser prints the AST for each file, or an error if parsing fails.

### Example

```bash
cargo run -- examples/app.tr
```

The `examples/app.tr` file shows the Trust DSL in action — importing use-cases from a git repo, declaring HTTP routes, and wiring them into a presenter and app.

---

## DSL overview

A `.tr` file is made up of declarations:

```
import CreateUseCase, DeleteUseCase from "git@github.com:org/repo.git"

http_routes my_routes {
    GET    "/" => CreateUseCase
    DELETE "/" => DeleteUseCase
}

presenter my_presenter {
    http {
        use my_routes
        on :3000
    }
}

app my_app: my_presenter
```

| Keyword       | Purpose                                         |
|---------------|-------------------------------------------------|
| `import`      | Pull use-cases from a git repository            |
| `http_routes` | Map HTTP verbs and paths to use-cases           |
| `presenter`   | Declare how a set of routes is exposed          |
| `app`         | Wire a presenter into a runnable application    |
