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

#[cfg(test)]
mod tests {
    use super::*;
    use prost::Message;
    use prost_types::{FileDescriptorProto, FileDescriptorSet};

    #[test]
    fn is_proto_generated_module_pb2() {
        assert!(is_proto_generated_module("service_pb2"));
        assert!(is_proto_generated_module("api.v1.service_pb2"));
        assert!(!is_proto_generated_module("service"));
        assert!(!is_proto_generated_module("service_pb2.something"));
    }

    #[test]
    fn is_proto_generated_module_grpc() {
        assert!(is_proto_generated_module("service_pb2_grpc"));
        assert!(is_proto_generated_module("api.v1.service_pb2_grpc"));
        assert!(!is_proto_generated_module("service_grpc"));
        assert!(!is_proto_generated_module("service_pb2_grpc.something"));
    }

    #[test]
    fn collect_generated_basenames_empty() {
        let fds = FileDescriptorSet { file: vec![] };
        let bytes = fds.encode_to_vec();

        let result = collect_generated_basenames_from_bytes(&bytes).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn collect_generated_basenames_single_file() {
        let file = FileDescriptorProto {
            name: Some("service/api.proto".to_string()),
            ..Default::default()
        };
        let fds = FileDescriptorSet { file: vec![file] };
        let bytes = fds.encode_to_vec();

        let result = collect_generated_basenames_from_bytes(&bytes).unwrap();
        let expected = ["api_pb2", "api_pb2_grpc"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(result, expected);
    }

    #[test]
    fn collect_generated_basenames_multiple_files() {
        let files = vec![
            FileDescriptorProto {
                name: Some("service/user.proto".to_string()),
                ..Default::default()
            },
            FileDescriptorProto {
                name: Some("api/payment.proto".to_string()),
                ..Default::default()
            },
            FileDescriptorProto {
                name: Some("common.proto".to_string()),
                ..Default::default()
            },
        ];
        let fds = FileDescriptorSet { file: files };
        let bytes = fds.encode_to_vec();

        let result = collect_generated_basenames_from_bytes(&bytes).unwrap();
        let expected = [
            "user_pb2",
            "user_pb2_grpc",
            "payment_pb2",
            "payment_pb2_grpc",
            "common_pb2",
            "common_pb2_grpc",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        assert_eq!(result, expected);
    }

    #[test]
    fn collect_generated_basenames_file_without_name() {
        let files = vec![
            FileDescriptorProto {
                name: Some("valid.proto".to_string()),
                ..Default::default()
            },
            FileDescriptorProto {
                name: None, // This file has no name
                ..Default::default()
            },
        ];
        let fds = FileDescriptorSet { file: files };
        let bytes = fds.encode_to_vec();

        let result = collect_generated_basenames_from_bytes(&bytes).unwrap();
        // Should only include basenames from files with valid names
        let expected = ["valid_pb2", "valid_pb2_grpc"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(result, expected);
    }

    #[test]
    fn collect_generated_basenames_nested_paths() {
        let file = FileDescriptorProto {
            name: Some("deeply/nested/path/service.proto".to_string()),
            ..Default::default()
        };
        let fds = FileDescriptorSet { file: vec![file] };
        let bytes = fds.encode_to_vec();

        let result = collect_generated_basenames_from_bytes(&bytes).unwrap();
        let expected = ["service_pb2", "service_pb2_grpc"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(result, expected);
    }

    #[test]
    fn collect_generated_basenames_invalid_bytes() {
        let invalid_bytes = b"invalid protobuf data";
        let result = collect_generated_basenames_from_bytes(invalid_bytes);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("decode FDS via prost-types failed")
        );
    }

    #[test]
    fn load_fds_from_bytes_valid() {
        // Create a minimal valid FileDescriptorSet
        let file = FileDescriptorProto {
            name: Some("test.proto".to_string()),
            package: Some("test".to_string()),
            ..Default::default()
        };
        let fds = FileDescriptorSet { file: vec![file] };
        let bytes = fds.encode_to_vec();

        let result = load_fds_from_bytes(&bytes);
        assert!(result.is_ok());
    }

    #[test]
    fn load_fds_from_bytes_invalid() {
        let invalid_bytes = b"not a valid protobuf";
        let result = load_fds_from_bytes(invalid_bytes);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("failed to decode FileDescriptorSet")
        );
    }
}
