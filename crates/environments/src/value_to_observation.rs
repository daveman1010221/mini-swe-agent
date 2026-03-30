//! Convert a `nu_protocol::Value` into an `Observation::Structured`.
//!
//! This is the only place the nu→JSON conversion happens for shell output.
//! The actual JSON serialization is deferred to `Observation::to_llm_content()`
//! so the `Value` travels as structured data inside the actor system.

use mswea_core::observation::Observation;
use nu_protocol::Value;

pub fn value_to_observation(value: Value, exit_code: i64) -> Observation {
    Observation::Structured { value, exit_code }
}
