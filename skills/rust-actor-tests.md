# Rust Actor Testing Pattern

To test a ractor Actor, always use this pattern:
```rust
#[tokio::test]
async fn test_actor_receives_message() {
    let (actor_ref, handle) = Actor::spawn(
        None,
        MyActor,
        MyActorArgs { ... }
    ).await.unwrap();

    actor_ref.send_message(MyActorMsg::SomeVariant {
        field: value
    }).unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    actor_ref.stop(None);
    handle.await.unwrap();
}
```

To observe outputs, subscribe an OutputPort before spawning:
```rust
let port = OutputPort::default();
let subscription = port.subscribe(|event| { ... });
let (actor_ref, handle) = Actor::spawn(None, MyActor, MyActorArgs {
    output: Arc::new(port),
    ...
}).await.unwrap();
```

NEVER do this — it won't compile because State is private:
```rust
let state = MyActorState { ... }; // WRONG
```

ALWAYS find the Args struct first:
```bash
rg -n 'pub struct.*Args\|pub enum.*Msg' src/myactor.rs
```

## Mock Actor Pattern

When an Actor's Arguments require TLS certs, env vars, or other complex
dependencies, use a no-op MockActor instead:
```rust
struct MockActor;

#[ractor::async_trait]
impl Actor for MockActor {
    type Msg = BrokerMessage;  // use whatever Msg type is needed
    type State = ();
    type Arguments = ();

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        _args: (),
    ) -> Result<(), ActorProcessingErr> {
        Ok(())
    }
}
```

Always spawn mock actors with `None` as the name to avoid `ActorAlreadyRegistered`
errors when tests run in parallel.

## MockActor for TcpClientMessage

When a test needs a mock for `TcpClientMessage`, define it inline in the test
file — do NOT create a separate mock file. Use the real `TcpClientMessage` type
as the Msg type, not a custom enum:
```rust
struct MockTcpClient;

#[ractor::async_trait]
impl Actor for MockTcpClient {
    type Msg = TcpClientMessage;
    type State = ();
    type Arguments = ();

    async fn pre_start(
        &self,
        _: ActorRef,
        _: (),
    ) -> Result {
        Ok(())
    }
}
```

Spawn it with `None` name and pass the `ActorRef<TcpClientMessage>` directly
to the actor under test.

## is_alive() does not exist on ActorRef

Never use `actor.is_alive()` — this method does not exist in ractor.
To verify an actor is still running after receiving messages, simply call
`actor.stop(None)` and `handle.await.unwrap()` — if either panics, the actor
crashed. That is sufficient proof of liveness.

## Before constructing ANY struct in tests

Read the struct definition before constructing it:
```bash
rg -n "pub struct StructName" src/ --type rust
sed -n '{start},{end}p' src/path/to/file.rs
```

Check for private fields. If ANY field is private, you CANNOT use struct
literal syntax. Use a constructor method instead:
```bash
rg -n "pub fn new\|pub fn from\|pub fn with" src/path/to/file.rs
```

Never assume all fields are public. Always check first.

## Before writing ANY new tests

Always count existing tests first — never rely on memory:
```bash
grep -c "^fn test_\|^async fn test_" tests/unit.rs
```

Use this count as your baseline. Your success condition is:
- All existing tests still pass
- New tests are appended, not rewritten
- `cargo test` reports zero failures

Never rewrite a test file from scratch. Always append to what exists.

## Before adding any dev-dependencies

ALWAYS check the workspace Cargo.toml first:
```bash
grep "{dep_name}" $WORKSPACE_ROOT/Cargo.toml
```

If the dependency is already declared in the workspace, use the workspace form:
```toml
[dev-dependencies]
tempfile = { workspace = true }
```

NEVER add a version directly like `tempfile = "3"` if it exists in the workspace.
If it is NOT in the workspace, add it to the workspace Cargo.toml first, then
reference it with `workspace = true` in the crate.

## When a crate has no tests/ directory yet

Some crates have no tests/ directory at all. Check first:
```bash
fd --type d 'tests' $WORKSPACE_ROOT/{crate_path}
```

If it doesn't exist, create it before writing any test file:
```bash
mkdir -p $WORKSPACE_ROOT/{crate_path}/tests
```

Each file under tests/ is a separate test binary. Create them individually:
- `tests/unit.rs` — for unit/serde/rkyv tests
- `tests/props.rs` — for proptests

Each must also be declared in Cargo.toml:
```toml
[[test]]
name = "unit"
path = "tests/unit.rs"

[[test]]
name = "props"
path = "tests/props.rs"
```

## Pure types crates (no actors)

For crates that only contain data types (enums, structs) with serde and rkyv
derives, the test pattern is simpler — no Actor::spawn needed.

ALWAYS check which derives are present before deciding what tests to write:
```bash
rg -n 'Archive\|#\[derive' $WORKSPACE_ROOT/{crate_path}/src/lib.rs
```

A crate with both serde and rkyv derives needs BOTH sets of roundtrip tests.
Missing either set means the task is incomplete — do not call mark_done.

### serde roundtrip
```rust
#[test]
fn test_my_type_serde_roundtrip() {
    let original = MyType::Variant { field: "value".to_string() };
    let json = serde_json::to_string(&original).unwrap();
    let decoded: MyType = serde_json::from_str(&json).unwrap();
    assert_eq!(original, decoded);
}
```

### rkyv roundtrip
```rust
#[test]
fn test_my_type_rkyv_roundtrip() {
    let original = MyType::Variant { field: "value".to_string() };
    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&original).unwrap();
    let decoded = rkyv::from_bytes::<MyType, rkyv::rancor::Error>(&bytes).unwrap();
    assert_eq!(original, decoded);
}
```

Note: if the type does not derive PartialEq, compare via serde_json serialization:
```rust
assert_eq!(
    serde_json::to_string(&original).unwrap(),
    serde_json::to_string(&decoded).unwrap()
);
```

Check for PartialEq before writing assertions:
```bash
rg -n 'PartialEq' $WORKSPACE_ROOT/{crate_path}/src/lib.rs
```

## Testing from_env() style functions

Never use `std::env::set_var` in tests — it mutates global process state and is
`unsafe` in Rust 2024. Instead, refactor the function to accept a lookup closure:
```rust
// In the source file:
impl MyConfig {
    pub fn from_env() -> Self {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    pub fn from_lookup<F: Fn(&str) -> Option<String>>(lookup: F) -> Self {
        let value = lookup("MY_VAR").unwrap_or_else(|| "default".to_string());
        Self { value }
    }
}
```

If the source file does not have `from_lookup`, add it — this is the correct
pattern for this codebase. `from_env()` becomes a thin wrapper, all callers
are unaffected.

Tests then use the closure directly — no env mutation, no unsafe, no serial_test:
```rust
#[test]
fn test_defaults() {
    let config = MyConfig::from_lookup(|_| None);
    assert_eq!(config.value, "default");
}

#[test]
fn test_override() {
    let config = MyConfig::from_lookup(|key| match key {
        "MY_VAR" => Some("custom".to_string()),
        _ => None,
    });
    assert_eq!(config.value, "custom");
}
```

## Success condition

Never use an exact test count as a success condition. Instead:
- Count existing tests first with grep -c
- Run cargo test and verify zero failures
- Confirm new tests appear in the output with ok status

## Integration test imports — always use the crate name, never `crate::`

Files under `tests/` are separate binaries. Inside them, `crate` refers to
the test binary itself, not the library being tested. Always import from the
library by name:

```rust
// WRONG — crate:: resolves to the test binary, not mswea-core
use crate::ExitStatus;

// CORRECT
use mswea_core::ExitStatus;
```

This applies to every `use` statement in `tests/unit.rs` and `tests/props.rs`.

## Check derives before writing any test for a type

Before writing a serde roundtrip test, verify the type actually derives
`Serialize`, `Deserialize`, and `PartialEq`:

```bash
rg -n '#\[derive' crates/core/src/error.rs
```

If `Serialize` is absent — skip serde roundtrip for that type.
If `Deserialize` is absent — skip serde roundtrip for that type.
If `PartialEq` is absent — do not use `assert_eq!` on decoded values.
Instead compare via serialization:

```rust
assert_eq!(
    serde_json::to_string(&original).unwrap(),
    serde_json::to_string(&decoded).unwrap()
);
```

`AgentError` is a `thiserror` error type — it intentionally has no serde
derives and no `PartialEq`. Do NOT write serde roundtrip tests for it.
Write tests only for `ExitStatus`, which has full derives.

## Check field types before constructing variants

Before constructing a variant with named fields, read the struct/enum
definition to verify field types:

```bash
rg -n -A5 'VariantName' crates/core/src/error.rs
```

`AgentError::Serialization` has `source: serde_json::Error` — you cannot
construct it with `Value::Null`. Only variants with simple field types
(String, u32, f64, bool) can be constructed inline in tests.
