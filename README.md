# python-proto-importer

[![Crates.io](https://img.shields.io/crates/v/python-proto-importer.svg)](https://crates.io/crates/python-proto-importer)
[![PyPI](https://img.shields.io/pypi/v/python-proto-importer.svg)](https://pypi.org/project/python-proto-importer/)
[![CI](https://github.com/K-dash/python-proto-importer/actions/workflows/ci.yml/badge.svg)](https://github.com/K-dash/python-proto-importer/actions)

Rust-based CLI to streamline Python gRPC/Protobuf workflows: generate code, stabilize imports, and run type checks in a single command. Ships as a PyPI package (via maturin) and as a Rust crate.

- **Backends**: `protoc` (v0.1), `buf generate` (planned v0.2)
- **Postprocess**: convert internal imports to relative; generate `__init__.py`
- **Typing**: optional `mypy-protobuf` / `mypy-grpc` emission
- **Verification**: import dry-run, optional mypy/pyright

For Japanese documentation, see: [docs/日本語 README](doc/README.ja.md)

## Table of Contents

- [Quick Start](#quick-start)
- [Commands](#commands)
- [Configuration](#configuration)
  - [Core Configuration](#core-configuration)
  - [Postprocess Configuration](#postprocess-configuration)
  - [Verification Configuration](#verification-configuration)
- [Configuration Examples](#configuration-examples)
- [Advanced Usage](#advanced-usage)
- [Limitations](#limitations)
- [Contributing](#contributing)
- [License](#license)

## Quick Start

```bash
pip install python-proto-importer
# or
cargo install python-proto-importer
```

Create a `pyproject.toml` with your configuration:

```toml
[tool.python_proto_importer]
backend = "protoc"
python_exe = "python3"
include = ["proto"]
inputs = ["proto/**/*.proto"]
out = "generated/python"
```

Run the build:

```bash
proto-importer build
```

## Commands

### `proto-importer doctor`

Check your environment for required tools (protoc, buf, grpc_tools, mypy, pyright).

### `proto-importer build [--pyproject PATH]`

Generate Python code from proto files, apply postprocessing, and run verification.

Options:

- `--pyproject PATH`: Path to pyproject.toml (default: `./pyproject.toml`)
- `--no-verify`: Skip verification after generation
- `--postprocess-only`: Skip generation, only run postprocessing (experimental)

### `proto-importer check [--pyproject PATH]`

Run verification only (import dry-run and type checks) without generation.

### `proto-importer clean [--pyproject PATH] --yes`

Remove generated output directory. Requires `--yes` confirmation.

## Configuration

All configuration is done through `pyproject.toml` under the `[tool.python_proto_importer]` section.

### Core Configuration

| Option       | Type    | Default              | Description                                                                                                                                                |
| ------------ | ------- | -------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `backend`    | string  | `"protoc"`           | Code generation backend. Currently only `"protoc"` is supported. `"buf"` planned for v0.2.                                                                 |
| `python_exe` | string  | `"python3"`          | Python executable to use for generation and verification. Can be `"python3"`, `"python"`, `"uv"` (fully tested), or a path like `".venv/bin/python"`.      |
| `include`    | array   | `["."]`              | Proto import paths (passed as `--proto_path` to protoc). Empty array defaults to `["."]`. See [Include Path Behavior](#include-path-behavior) for details. |
| `inputs`     | array   | `[]`                 | Glob patterns for proto files to generate. Example: `["proto/**/*.proto"]`. Files are filtered by `include` paths.                                         |
| `out`        | string  | `"generated/python"` | Output directory for generated Python files.                                                                                                               |
| `mypy`       | boolean | `false`              | Generate mypy type stubs (`.pyi` files) using `mypy-protobuf`.                                                                                             |
| `mypy_grpc`  | boolean | `false`              | Generate gRPC mypy stubs (`_grpc.pyi` files) using `mypy-grpc`.                                                                                            |

#### Include Path Behavior

The `include` option controls proto import paths and has important interactions with `inputs`:

1. **Default Behavior**: If `include` is empty or not specified, it defaults to `["."]` (current directory).

2. **Path Resolution**: Each path in `include` is passed to protoc as `--proto_path`. Proto files can only import other protos within these paths.

3. **Input Filtering**: Files matched by `inputs` globs are automatically filtered to only include those under `include` paths. This prevents protoc errors when globs match files outside the include paths.

4. **Output Structure**: Generated files maintain the directory structure relative to the `include` path. For example:

   - With `include = ["proto"]` and a file at `proto/service/api.proto`
   - Output will be at `{out}/service/api_pb2.py`

5. **Multiple Include Paths**: When specifying multiple paths like `include = ["proto/common", "proto/services"]`, be aware that files with the same relative path may cause conflicts.

**Examples:**

```toml
# Simple case - all protos under proto/ directory
include = ["proto"]
inputs = ["proto/**/*.proto"]

# Multiple include paths - useful for separate proto roots
include = ["common/proto", "services/proto"]
inputs = ["**/*.proto"]

# Selective generation - only specific services
include = ["."]  # Use current directory to avoid path conflicts
inputs = ["proto/payment/**/*.proto", "proto/user/**/*.proto"]

# Alternative proto structure
include = ["api/definitions"]
inputs = ["api/definitions/**/*.proto"]
```

### Postprocess Configuration

The `postprocess` table controls post-generation transformations:

| Option             | Type    | Default   | Description                                                                                     |
| ------------------ | ------- | --------- | ----------------------------------------------------------------------------------------------- |
| `relative_imports` | boolean | `true`    | Convert absolute imports to relative imports within generated files.                            |
| `fix_pyi`          | boolean | `true`    | Fix type annotations in `.pyi` files (currently reserved for future use).                       |
| `create_package`   | boolean | `true`    | Create `__init__.py` files in all directories. Set to `false` for namespace packages (PEP 420). |
| `exclude_google`   | boolean | `true`    | Exclude `google.protobuf` imports from relative import conversion.                              |
| `pyright_header`   | boolean | `false`   | Add Pyright suppression header to generated `_pb2.py` and `_pb2_grpc.py` files.                 |
| `module_suffixes`  | array   | See below | File suffixes to process during postprocessing.                                                 |

Default `module_suffixes`:

```toml
module_suffixes = ["_pb2.py", "_pb2.pyi", "_pb2_grpc.py", "_pb2_grpc.pyi"]
```

### Verification Configuration

The `[tool.python_proto_importer.verify]` section configures optional verification commands:

| Option        | Type  | Default | Description                                                                     |
| ------------- | ----- | ------- | ------------------------------------------------------------------------------- |
| `mypy_cmd`    | array | `null`  | Command to run mypy type checking. Example: `["mypy", "--strict", "generated"]` |
| `pyright_cmd` | array | `null`  | Command to run pyright type checking. Example: `["pyright", "generated"]`       |

**Important Notes:**

1. **Import Dry-run**: Always performed automatically. The tool attempts to import all generated Python modules to ensure they're valid.

2. **Type Checking**: Only runs if configured. The tools (mypy/pyright) must be available in your environment.

3. **Command Arrays**: Commands are specified as arrays where the first element is the executable and remaining elements are arguments.

**Examples:**

```toml
[tool.python_proto_importer.verify]
# Using uv to run type checkers
mypy_cmd = ["uv", "run", "mypy", "--strict", "generated/python"]
pyright_cmd = ["uv", "run", "pyright", "generated/python"]

# Direct execution
mypy_cmd = ["mypy", "--config-file", "mypy.ini", "generated"]

# Check only .pyi files with pyright
pyright_cmd = ["pyright", "generated/**/*.pyi"]

# Exclude generated gRPC files from mypy strict checking
mypy_cmd = ["mypy", "--strict", "--exclude", ".*_grpc\\.py$", "generated"]
```

## Configuration Examples

### Minimal Configuration

```toml
[tool.python_proto_importer]
backend = "protoc"
inputs = ["proto/**/*.proto"]
out = "generated"
```

### Full-Featured Configuration

```toml
[tool.python_proto_importer]
backend = "protoc"
python_exe = ".venv/bin/python"
include = ["proto"]
inputs = ["proto/**/*.proto"]
out = "src/generated"
mypy = true
mypy_grpc = true

[tool.python_proto_importer.postprocess]
relative_imports = true
fix_pyi = true
create_package = true
exclude_google = true
pyright_header = true

[tool.python_proto_importer.verify]
mypy_cmd = ["uv", "run", "mypy", "--strict", "--exclude", ".*_grpc\\.py$", "src/generated"]
pyright_cmd = ["uv", "run", "pyright", "src/generated/**/*.pyi"]
```

### Namespace Package Configuration (PEP 420)

```toml
[tool.python_proto_importer]
backend = "protoc"
include = ["proto"]
inputs = ["proto/**/*.proto"]
out = "generated"

[tool.python_proto_importer.postprocess]
create_package = false  # Don't create __init__.py files
```

### Selective Service Generation

```toml
[tool.python_proto_importer]
backend = "protoc"
include = ["."]
# Only generate specific services
inputs = [
    "proto/authentication/**/*.proto",
    "proto/user_management/**/*.proto"
]
out = "services/generated"
```

### Custom Directory Structure

```toml
[tool.python_proto_importer]
backend = "protoc"
# For non-standard proto locations
include = ["api/v1/definitions"]
inputs = ["api/v1/definitions/**/*.proto"]
out = "build/python/api"
```

## Advanced Usage

### Using with uv

[uv](https://github.com/astral-sh/uv) is a fast Python package manager that can replace pip and virtualenv:

```toml
[tool.python_proto_importer]
python_exe = "uv"  # or ".venv/bin/python" if using uv venv
# ... rest of config

[tool.python_proto_importer.verify]
mypy_cmd = ["uv", "run", "mypy", "--strict", "generated"]
```

### CI/CD Integration

```yaml
# GitHub Actions example
- name: Install dependencies
  run: |
    pip install python-proto-importer
    pip install grpcio-tools mypy-protobuf

- name: Generate Python code from protos
  run: proto-importer build

- name: Run tests
  run: pytest tests/
```

### Understanding `include` vs `inputs`

One of the most important concepts to understand when configuring python-proto-importer is the difference between `include` and `inputs`:

#### 🗂️ `include` - "Where to Look" (Search Paths)

Specifies **where** the protobuf compiler (protoc) should **search** for `.proto` files.

#### 📄 `inputs` - "What to Compile" (Target Files)

Specifies **which** `.proto` files you want to **compile** using glob patterns.

#### 🏗️ Example Project Structure

```
my-project/
├── api/
│   ├── user/
│   │   └── user.proto          # Want to compile this
│   └── order/
│       └── order.proto         # Want to compile this
├── third_party/
│   └── google/
│       └── protobuf/
│           └── timestamp.proto # Referenced as dependency
└── generated/                  # Output directory
```

#### ⚙️ Configuration Example

```toml
[tool.python_proto_importer]
include = ["api", "third_party"]           # Search paths
inputs = ["api/**/*.proto"]                # Files to compile
out = "generated"
```

#### 🔍 How It Works

1. **`inputs`**: `api/**/*.proto` → finds `user.proto` and `order.proto`
2. **`include`**: Sets `api` and `third_party` as search paths
3. **Compilation**:
   - When compiling `user.proto`, if it contains `import "google/protobuf/timestamp.proto"`
   - The compiler can automatically find `third_party/google/protobuf/timestamp.proto`

#### 🚫 Common Mistakes

**❌ Wrong Pattern:**

```toml
# Wrong: Including dependencies in inputs causes them to be generated
inputs = ["api/**/*.proto", "third_party/**/*.proto"]  # Generates unwanted files
include = ["api"]                                      # Missing search paths
```

**✅ Correct Pattern:**

```toml
# Correct: Only compile what you need, but include all search paths
inputs = ["api/**/*.proto"]                    # Only compile your API files
include = ["api", "third_party"]               # Include all paths for dependencies
```

#### 🎯 Key Takeaway

- **`include`** = Compiler's "eyes" (what it can see)
- **`inputs`** = Compiler's "hands" (what it grabs and compiles)

Dependencies are **not compiled** (excluded from `inputs`) but **must be searchable** (included in `include`).

This approach ensures you **generate only the files you need** while **properly resolving all dependencies**.

### Handling Complex Proto Dependencies

When dealing with complex proto dependencies across multiple directories:

```toml
[tool.python_proto_importer]
# Include all necessary proto roots
include = [
    ".",
    "third_party/proto",
    "vendor/proto"
]
# Use specific patterns to avoid conflicts
inputs = [
    "src/proto/**/*.proto",
    "third_party/proto/specific_service/**/*.proto"
]
out = "generated"
```

## Limitations

- **v0.1 limitations**:
  - Only `protoc` backend is supported. `buf generate` support is planned for v0.2.
  - Import rewriting targets common `_pb2(_grpc)?.py[i]` patterns; broader coverage is added incrementally with tests.
- **Known behaviors**:
  - When using multiple `include` paths with files of the same name, protoc may report "shadowing" errors. Use selective `inputs` patterns to avoid this.
  - Generated file structure follows protoc conventions: files are placed relative to their `--proto_path`.
  - Type checkers (mypy/pyright) must be installed separately and available in PATH or the Python environment.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup and guidelines.

## License

Apache-2.0. See LICENSE for details.
