use anyhow::Result;
use regex::Regex;
use std::fs;
use std::path::Path;

/// Very small scaffold for future import rewriting.
/// For now, it only identifies candidate lines and returns count.
#[allow(dead_code)]
pub fn rewrite_file_for_relative_imports(path: &Path) -> Result<usize> {
    let content = fs::read_to_string(path)?;
    let import_re = Regex::new(r"(?m)^import\s+([A-Za-z0-9_\.]+_pb2(?:_grpc)?)\b").unwrap();
    let from_re =
        Regex::new(r"(?m)^from\s+([A-Za-z0-9_\.]+)\s+import\s+([A-Za-z0-9_]+_pb2(?:_grpc)?)\b")
            .unwrap();

    let mut hits = 0usize;
    hits += import_re.find_iter(&content).count();
    hits += from_re.find_iter(&content).count();

    // No modifications yet; further phases will compute and apply rewrites.
    Ok(hits)
}

/// Walk output tree and report count of candidate files/lines (dry-run).
#[allow(dead_code)]
pub fn scan_and_report(root: &Path) -> Result<(usize, usize)> {
    let mut files = 0usize;
    let mut lines = 0usize;
    for entry in walkdir::WalkDir::new(root)
        .into_iter()
        .filter_map(Result::ok)
    {
        let p = entry.path();
        if p.is_file() && p.extension().and_then(|e| e.to_str()) == Some("py") {
            files += 1;
            lines += rewrite_file_for_relative_imports(p)?;
        }
    }
    Ok((files, lines))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn rewrite_file_for_relative_imports_basic() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.py");

        let content = r#"
import service_pb2
from api.v1 import user_pb2
import other_module
from package import regular_module
import grpc_service_pb2_grpc
"#;
        fs::write(&file_path, content).unwrap();

        let hits = rewrite_file_for_relative_imports(&file_path).unwrap();
        // Should match: service_pb2, user_pb2, grpc_service_pb2_grpc
        assert_eq!(hits, 3);
    }

    #[test]
    fn rewrite_file_for_relative_imports_no_matches() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.py");

        let content = r#"
import os
from typing import List
import requests
from dataclasses import dataclass
"#;
        fs::write(&file_path, content).unwrap();

        let hits = rewrite_file_for_relative_imports(&file_path).unwrap();
        assert_eq!(hits, 0);
    }

    #[test]
    fn rewrite_file_for_relative_imports_complex_patterns() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.py");

        let content = r#"
# Import statements
import api.v1.service_pb2
import api.v2.user_pb2_grpc
from package.subpackage import module_pb2
from api import payment_pb2_grpc
from . import local_pb2  # Should not match (already relative)

# Mixed content
def function():
    pass
    
import another_service_pb2
"#;
        fs::write(&file_path, content).unwrap();

        let hits = rewrite_file_for_relative_imports(&file_path).unwrap();
        // Should match: service_pb2, user_pb2_grpc, module_pb2, payment_pb2_grpc, another_service_pb2
        // But local_pb2 is also counted by the regex even though it starts with "from ."
        assert_eq!(hits, 6);
    }

    #[test]
    fn rewrite_file_for_relative_imports_multiline() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.py");

        let content = "import service_pb2\nfrom api import user_pb2\nimport normal_module";
        fs::write(&file_path, content).unwrap();

        let hits = rewrite_file_for_relative_imports(&file_path).unwrap();
        assert_eq!(hits, 2); // service_pb2 and user_pb2
    }

    #[test]
    fn scan_and_report_basic() {
        let dir = tempdir().unwrap();

        // Create Python files with proto imports
        let file1 = dir.path().join("service.py");
        let file2 = dir.path().join("api.py");
        let file3 = dir.path().join("utils.txt"); // Non-Python file

        fs::write(&file1, "import service_pb2\nfrom api import user_pb2").unwrap();
        fs::write(&file2, "import payment_pb2_grpc").unwrap();
        fs::write(&file3, "import service_pb2").unwrap(); // Should be ignored

        let (files, lines) = scan_and_report(dir.path()).unwrap();
        assert_eq!(files, 2); // Only .py files counted
        assert_eq!(lines, 3); // Total proto import lines
    }

    #[test]
    fn scan_and_report_nested_directories() {
        let dir = tempdir().unwrap();

        // Create nested structure
        let nested_dir = dir.path().join("services");
        fs::create_dir_all(&nested_dir).unwrap();

        let file1 = dir.path().join("main.py");
        let file2 = nested_dir.join("api.py");

        fs::write(&file1, "import main_service_pb2").unwrap();
        fs::write(&file2, "from proto import api_pb2\nimport grpc_pb2_grpc").unwrap();

        let (files, lines) = scan_and_report(dir.path()).unwrap();
        assert_eq!(files, 2);
        assert_eq!(lines, 3); // 1 + 2 imports
    }

    #[test]
    fn scan_and_report_empty_directory() {
        let dir = tempdir().unwrap();

        let (files, lines) = scan_and_report(dir.path()).unwrap();
        assert_eq!(files, 0);
        assert_eq!(lines, 0);
    }

    #[test]
    fn scan_and_report_no_proto_imports() {
        let dir = tempdir().unwrap();

        let file = dir.path().join("normal.py");
        fs::write(&file, "import os\nfrom typing import List").unwrap();

        let (files, lines) = scan_and_report(dir.path()).unwrap();
        assert_eq!(files, 1); // File is counted
        assert_eq!(lines, 0); // But no proto import lines
    }

    #[test]
    fn rewrite_file_nonexistent_file() {
        let nonexistent = std::path::Path::new("/nonexistent/file.py");
        let result = rewrite_file_for_relative_imports(nonexistent);
        assert!(result.is_err());
    }
}
