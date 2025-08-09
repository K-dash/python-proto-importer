# python-proto-importer

[![Crates.io](https://img.shields.io/crates/v/python-proto-importer.svg)](https://crates.io/crates/python-proto-importer)
[![PyPI](https://img.shields.io/pypi/v/python-proto-importer.svg)](https://pypi.org/project/python-proto-importer/)
[![CI](https://github.com/K-dash/python-proto-importer/actions/workflows/ci.yml/badge.svg)](https://github.com/K-dash/python-proto-importer/actions)

Rust-based CLI to streamline Python gRPC/Protobuf workflows: generate code, stabilize imports, and run type checks in a single command. Ships as a PyPI package (via maturin) and as a Rust crate.

- **Backends**: `protoc` (v0.1), `buf generate` (planned v0.2)
- **Postprocess**: convert internal imports to relative; generate `__init__.py`
- **Typing**: optional `mypy-protobuf` / `mypy-grpc` emission
- **Verification**: import dry-run, optional mypy/pyright

For Japanese documentation, see: [docs/日本語README](doc/README.ja.md)

## Quick start

```bash
pip install python-proto-importer
# or
cargo install python-proto-importer
```

## pyproject.toml example (protoc backend)

```toml
[tool.python_proto_importer]
backend = "protoc"
python_exe = "python3" # or "uv"
include = ["proto"]
inputs = ["proto/**/*.proto"]
out = "generated/python"
# Optional type stub generation
mypy = true
mypy_grpc = true
postprocess = { protoletariat = true, fix_pyi = true, create_package = true, exclude_google = true }

[tool.python_proto_importer.verify]
# Keep minimal here; recommend project-specific settings
mypy_cmd = ["uv", "run", "mypy", "--strict", "generated/python"]
pyright_cmd = ["uv", "run", "pyright", "generated/python"]
```

## Buf backend example (planned v0.2)

`buf generate` support is planned for v0.2. A tentative example configuration:

```toml
[tool.python_proto_importer]
backend = "buf"
buf_gen_yaml = "buf.gen.yaml"
postprocess = { protoletariat = true, fix_pyi = true, create_package = true, exclude_google = true }

[tool.python_proto_importer.verify]
# Adjust to your project
mypy_cmd = ["uv", "run", "mypy", "--strict", "gen/python"]
```

## Contribute

[CONTRIBUTING.md](CONTRIBUTING.md).

## Limitations

- v0.1 supports `protoc` backend only. `buf generate` support is planned in v0.2.
- Import rewriting targets common `_pb2(_grpc)?.py[ i]` patterns; broader coverage is added incrementally with tests.
- Type checks (`mypy`/`pyright`) run only if configured; they require the tools to be available in PATH.
- Namespace packages (PEP 420): we default to generating `__init__.py` to ensure importability; can be disabled via config.

## License

Apache-2.0. This project is an independent Rust re-implementation inspired by the behavior of existing OSS tools (e.g., Protoletariat, fix-protobuf-imports). Please see LICENSE for details.
