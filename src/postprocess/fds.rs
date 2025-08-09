use anyhow::{Context, Result};
use prost_reflect::DescriptorPool;

/// Load a FileDescriptorSet (binary) and return a DescriptorPool
#[allow(dead_code)]
pub fn load_fds_from_bytes(bytes: &[u8]) -> Result<DescriptorPool> {
    let pool = DescriptorPool::decode(bytes).context("failed to decode FileDescriptorSet")?;
    Ok(pool)
}

/// Given a pool and a relative module path, determine if an import target
/// corresponds to a .proto-derived module according to the pool entries.
/// For now, this is a placeholder returning true if suffix matches _pb2 or _pb2_grpc.
#[allow(dead_code)]
pub fn is_proto_generated_module(module: &str) -> bool {
    module.ends_with("_pb2") || module.ends_with("_pb2_grpc")
}
