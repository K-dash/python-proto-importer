use std::fs;
use std::io::Write;
use std::process::Command;

fn has_python_grpc_tools(py: &str) -> bool {
    let out = Command::new(py)
        .args([
            "-c",
            "import pkgutil; import sys; sys.exit(0 if pkgutil.find_loader('grpc_tools') else 1)",
        ])
        .output()
        .ok();
    matches!(out, Some(o) if o.status.success())
}

#[test]
fn e2e_protoc_build_and_rewrite_smoke() {
    // Only run when explicitly enabled
    if std::env::var("E2E_RUN").ok().as_deref() != Some("1") {
        eprintln!("skipping e2e (set E2E_RUN=1 to enable)");
        return;
    }

    // Probe python
    let py = std::env::var("PROTO_IMPORTER_PY")
        .ok()
        .unwrap_or_else(|| "python3".to_string());
    if !has_python_grpc_tools(&py) {
        eprintln!("skipping e2e (grpc_tools not available)");
        return;
    }

    let tdir = tempfile::tempdir().unwrap();
    let root = tdir.path();
    let proto_dir = root.join("proto");
    let out_dir = root.join("gen");
    fs::create_dir_all(&proto_dir).unwrap();

    // thing2.proto
    fs::write(
        proto_dir.join("thing2.proto"),
        r#"syntax = "proto3";
        package things;
        message Thing2 { string data = 1; }
        "#,
    )
    .unwrap();

    // thing1.proto imports thing2.proto
    fs::write(
        proto_dir.join("thing1.proto"),
        r#"syntax = "proto3";
        import "thing2.proto";
        package things;
        message Thing1 { Thing2 thing2 = 1; }
        "#,
    )
    .unwrap();

    // pyproject.toml
    let pyproject = root.join("pyproject.toml");
    let mut f = fs::File::create(&pyproject).unwrap();
    write!(
        f,
        r#"[tool.python_proto_importer]
backend = "protoc"
python_exe = "{}"
include = ["{}"]
inputs = ["{}"]
out = "{}"
postprocess = {{ protoletariat = true, fix_pyi = true, create_package = true, exclude_google = true }}

[tool.python_proto_importer.verify]
# keep verify minimal in e2e to avoid env constraints
"#,
        py,
        proto_dir.display(),
        proto_dir.join("thing1.proto").display(),
        out_dir.display()
    )
    .unwrap();

    // Run build via compiled binary
    let bin = env!("CARGO_BIN_EXE_python-proto-importer");
    let status = Command::new(bin)
        .current_dir(root)
        .args([
            "build",
            "--pyproject",
            pyproject.to_str().unwrap(),
            "--no-verify",
        ])
        .status()
        .expect("failed to run proto-importer build");
    assert!(status.success());

    // Assert generated rewrite exists
    let thing1_py = out_dir.join("things").join("thing1_pb2.py");
    let content = fs::read_to_string(&thing1_py).unwrap();
    assert!(
        content.contains("from . import thing2_pb2"),
        "rewrite not applied: {}",
        content
    );
}
