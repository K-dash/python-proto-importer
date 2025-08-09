use anyhow::{Context, Result};
use prost::Message;
use prost_reflect::DescriptorPool;
use prost_types::FileDescriptorSet;
use std::collections::HashSet;
use std::path::Path;

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

/// Decode bytes into FileDescriptorSet and collect generated module basenames
/// like "foo_pb2", "foo_pb2_grpc" for each file in the set.
pub fn collect_generated_basenames_from_bytes(bytes: &[u8]) -> Result<HashSet<String>> {
    let fds = FileDescriptorSet::decode(bytes).context("decode FDS via prost-types failed")?;
    let mut set = HashSet::new();
    for file in fds.file {
        if let Some(name) = file.name {
            if let Some(stem) = Path::new(&name).file_stem().and_then(|s| s.to_str()) {
                set.insert(format!("{stem}_pb2"));
                set.insert(format!("{stem}_pb2_grpc"));
            }
        }
    }
    Ok(set)
}
