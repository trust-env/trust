# Current Limitations

## External Rust Type Reuse

- Directly importing an existing Rust `.rs` type/class into Trust DSL is not supported.
- Trust imports operate on exported Trust types from `.tr` modules.

## Git Imports

- Git-style module import syntax is recognized by the parser.
- Code generation currently rejects git imports.

## AI Provider Support

- `anthropic` is the supported provider path in current implementation.

## Plugin Constraints

- Plugins must match the expected ABI version.
- Plugin-generated output file names are restricted to safe, single-segment names.

## Regex Type Semantics

- `Regex<...>` syntax exists as a declared type form.
- Validation semantics are not enforced as a standalone runtime validator by DSL itself.
