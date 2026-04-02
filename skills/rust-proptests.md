# Rust Proptest Pattern

Proptests use the `proptest` crate to generate arbitrary inputs. They are NOT
regular loops with hardcoded values. Always use this exact pattern:
```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn test_name(input in any::<Vec<u8>>()) {
        // test body
        prop_assert!(some_condition);
        prop_assert_eq!(a, b);
    }
}
```

## Before using proptest in a new crate

ALWAYS check `[dev-dependencies]` in the crate's Cargo.toml first:
```bash
grep "proptest" Cargo.toml
```

If missing, add it before writing any proptest code:
```toml
[dev-dependencies]
proptest = { workspace = true }
```

Never write proptest code and then discover it won't compile because
the dependency is missing.

## Common strategies
```rust
// Non-empty string — use regex, NOT .filter()
fn test(s in ".+") { ... }

// Arbitrary string
fn test(s in any::<String>()) { ... }

// Arbitrary bytes
fn test(bytes in any::<Vec<u8>>()) { ... }

// String matching a regex
fn test(topic in "[a-zA-Z0-9._/-]{1,64}") { ... }

// Bounded integer
fn test(n in 0u64..1000) { ... }

// Arbitrary vec of strings
fn test(topics in prop::collection::vec(".*", 0..10)) { ... }

// One of a fixed set
fn test(fmt in prop_oneof![Just("json"), Just("text")]) { ... }

// Printable unicode string
fn test(s in "\\PC*") { ... }
```

## Rules

- ALWAYS use `proptest! { }` macro — never use regular for loops as "proptests"
- ALWAYS use `prop_assert!` and `prop_assert_eq!` inside proptest — never use `assert!`
- The function signature inside `proptest!` uses `in` to bind strategies to names
- proptest runs 256 cases by default — no need to loop manually
- For serde roundtrips: serialize then deserialize, prop_assert_eq!(original, deserialized)
- For panic-safety tests: just call the function, proptest catches panics automatically

## WRONG — do not use .filter() on a strategy:
```rust
// DOES NOT COMPILE — .filter() is an iterator method, not a proptest strategy method
fn test(s in any::<String>().filter(|s| !s.is_empty())) { ... }
```

## RIGHT — use a regex strategy for non-empty strings:
```rust
fn test(s in ".+") { ... }
```

## WRONG — this is NOT a proptest:
```rust
#[test]
fn prop_something() {
    for value in ["a", "b", "c"] {
        // This is just a regular test with a misleading name
        assert!(do_thing(value).is_ok());
    }
}
```

## RIGHT:
```rust
proptest! {
    #[test]
    fn prop_something(value in any::<String>()) {
        prop_assert!(do_thing(&value).is_ok());
    }
}
```

## File structure for proptest files

When creating a separate `tests/props.rs` integration test binary, the file
stands alone — do NOT declare it as a module inside another test file.
Each file directly under `tests/` is its own test binary in Cargo.

WRONG — do not put this in tests/unit.rs:
```rust
mod props; // This is wrong — props.rs is its own binary
```

RIGHT — tests/props.rs is a standalone file with its own imports:
```rust
// tests/props.rs
use proptest::prelude::*;
use my_crate::MyType;

proptest! {
    #[test]
    fn prop_my_test(input in any::<String>()) { ... }
}
```

## Success condition

Never use an exact test count as a success condition. Instead:
- Run `cargo test --package {crate} --test props` and verify zero failures
- All tests must show `ok`, none `FAILED`
