//! Cluster serialization support for TaskActor RPC types.
//!
//! Implements `ractor::BytesConvertable` for all request/response types that
//! cross the ractor cluster boundary between the mswea-core node and the
//! nu-plugin-mswea node.
//!
//! We use rkyv for serialization — zero-copy, fast on localhost IPC.

use rkyv::rancor::Error as RkyvError;

use crate::task::{
    AdvanceRequest, AdvanceResponse,
    DeferTaskRequest, DeferTaskResponse,
    HaltRequest, HaltResponse,
    LoadTaskRequest, LoadTaskResponse,
    RecordAttemptRequest, RecordAttemptResponse,
    RecordOrientRequest, RecordOrientResponse,
    TaskStateData, TaskStateResponse,
    WriteCoveragePlanRequest, WriteCoveragePlanResponse,
};

macro_rules! impl_bytes_convertable {
    ($t:ty) => {
        impl ractor::BytesConvertable for $t {
            fn into_bytes(self) -> Vec<u8> {
                rkyv::to_bytes::<RkyvError>(&self)
                    .expect("rkyv serialization cannot fail for cluster messages")
                    .to_vec()
            }
            fn from_bytes(bytes: Vec<u8>) -> Self {
                rkyv::from_bytes::<Self, RkyvError>(&bytes)
                    .expect("rkyv deserialization cannot fail for cluster messages")
            }
        }
    };
}

impl_bytes_convertable!(AdvanceRequest);
impl_bytes_convertable!(AdvanceResponse);
impl_bytes_convertable!(DeferTaskRequest);
impl_bytes_convertable!(DeferTaskResponse);
impl_bytes_convertable!(HaltRequest);
impl_bytes_convertable!(HaltResponse);
impl_bytes_convertable!(LoadTaskRequest);
impl_bytes_convertable!(LoadTaskResponse);
impl_bytes_convertable!(RecordAttemptRequest);
impl_bytes_convertable!(RecordAttemptResponse);
impl_bytes_convertable!(RecordOrientRequest);
impl_bytes_convertable!(RecordOrientResponse);
impl_bytes_convertable!(TaskStateData);
impl_bytes_convertable!(TaskStateResponse);
impl_bytes_convertable!(WriteCoveragePlanRequest);
impl_bytes_convertable!(WriteCoveragePlanResponse);
