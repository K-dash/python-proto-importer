import shutil
import subprocess
from pathlib import Path

import pytest

HERE = Path(__file__).resolve().parent.parent
ROOT = HERE
# Use relative path from python_e2e to project root
PROJECT_ROOT = HERE.parent
BIN = PROJECT_ROOT / "target" / "release" / "python-proto-importer"


def clean_generated_dirs():
    """Clean all possible generated directories"""
    dirs_to_clean = [
        ROOT / "generated",
        ROOT / "generated_alt",
        ROOT / "src" / "proto_importer_e2e" / "generated",
    ]
    for dir_path in dirs_to_clean:
        if dir_path.exists():
            shutil.rmtree(dir_path)


@pytest.fixture(autouse=True)
def cleanup():
    """Clean generated files before and after each test"""
    clean_generated_dirs()
    yield
    clean_generated_dirs()


def run_build(config_path: Path) -> subprocess.CompletedProcess:
    """Run the build command with given config"""
    result = subprocess.run(
        ["uv", "run", str(BIN), "build", "--pyproject", str(config_path)],
        cwd=ROOT,
        capture_output=True,
        text=True,
    )
    print(result.stdout)
    print(result.stderr)
    return result


def verify_basic_structure(gen_path: Path, has_init: bool = True):
    """Verify basic generated file structure"""
    # Payment service files
    assert (gen_path / "proto" / "payment" / "payment_pb2.py").exists()
    assert (gen_path / "proto" / "payment" / "payment_pb2_grpc.py").exists()
    assert (gen_path / "proto" / "payment" / "types_pb2.py").exists()
    assert (gen_path / "proto" / "payment" / "types_pb2_grpc.py").exists()
    
    # User service files
    assert (gen_path / "proto" / "user" / "user_pb2.py").exists()
    assert (gen_path / "proto" / "user" / "user_pb2_grpc.py").exists()
    assert (gen_path / "proto" / "user" / "types_pb2.py").exists()
    assert (gen_path / "proto" / "user" / "types_pb2_grpc.py").exists()
    
    # Inventory service files
    assert (gen_path / "proto" / "inventory" / "inventory_pb2.py").exists()
    assert (gen_path / "proto" / "inventory" / "inventory_pb2_grpc.py").exists()
    
    # Check __init__.py files based on config
    if has_init:
        assert (gen_path / "proto" / "__init__.py").exists()
        assert (gen_path / "proto" / "payment" / "__init__.py").exists()
        assert (gen_path / "proto" / "user" / "__init__.py").exists()
        assert (gen_path / "proto" / "inventory" / "__init__.py").exists()
        assert (gen_path / "__init__.py").exists()
    else:
        assert not (gen_path / "proto" / "__init__.py").exists()
        assert not (gen_path / "proto" / "payment" / "__init__.py").exists()


def verify_type_stubs(gen_path: Path, has_mypy: bool, has_mypy_grpc: bool):
    """Verify type stub files based on config"""
    if has_mypy:
        # Check for .pyi files
        assert (gen_path / "proto" / "payment" / "payment_pb2.pyi").exists()
        assert (gen_path / "proto" / "payment" / "types_pb2.pyi").exists()
        assert (gen_path / "proto" / "user" / "user_pb2.pyi").exists()
        assert (gen_path / "proto" / "user" / "types_pb2.pyi").exists()
        assert (gen_path / "proto" / "inventory" / "inventory_pb2.pyi").exists()
    else:
        # No .pyi files should be generated
        assert not (gen_path / "proto" / "payment" / "payment_pb2.pyi").exists()
    
    if has_mypy_grpc:
        # Check for grpc .pyi files  
        assert (gen_path / "proto" / "payment" / "payment_pb2_grpc.pyi").exists()
        assert (gen_path / "proto" / "user" / "user_pb2_grpc.pyi").exists()
        assert (gen_path / "proto" / "inventory" / "inventory_pb2_grpc.pyi").exists()
    else:
        # No grpc .pyi files
        assert not (gen_path / "proto" / "payment" / "payment_pb2_grpc.pyi").exists()


def verify_pyright_header(gen_path: Path, has_header: bool):
    """Check if pyright header is present in generated files"""
    test_file = gen_path / "proto" / "payment" / "payment_pb2.py"
    if test_file.exists():
        content = test_file.read_text()
        has_pyright = "# pyright:" in content
        assert has_pyright == has_header


def verify_relative_import_rewrite(gen_path: Path):
    """Verify that absolute imports were rewritten to relative imports."""
    target = gen_path / "proto" / "payment" / "payment_pb2.py"
    assert target.exists(), f"{target} not found"
    content = target.read_text(encoding="utf-8")
    # should import sibling module relatively
    assert "from . import types_pb2" in content
    # absolute form should not remain
    assert "import proto.payment.types_pb2" not in content


@pytest.mark.parametrize("config_name,expected", [
    ("config_minimal", {
        "out": "generated/minimal",
        "has_mypy": False,
        "has_mypy_grpc": False,
        "has_init": True,
        "has_pyright_header": False,
    }),
    ("config_mypy_only", {
        "out": "generated/mypy_only", 
        "has_mypy": True,
        "has_mypy_grpc": False,
        "has_init": True,
        "has_pyright_header": False,
    }),
    ("config_full", {
        "out": "generated/full",
        "has_mypy": True,
        "has_mypy_grpc": True,
        "has_init": True,
        "has_pyright_header": True,
    }),
    ("config_no_package", {
        "out": "generated/no_package",
        "has_mypy": True,
        "has_mypy_grpc": False,
        "has_init": False,
        "has_pyright_header": False,
    }),
    ("config_custom_out", {
        "out": "generated_alt/python",
        "has_mypy": False,
        "has_mypy_grpc": False,
        "has_init": True,
        "has_pyright_header": False,
    }),
    ("config_uv_python_exe", {
        "out": "generated/uv_test",
        "has_mypy": False,
        "has_mypy_grpc": False,
        "has_init": True,
        "has_pyright_header": False,
    }),
])
def test_with_config(config_name, expected):
    """Test build with different configuration files"""
    config_path = ROOT / "configs" / f"{config_name}.toml"
    assert config_path.exists(), f"Config file {config_path} not found"
    
    # Run build
    result = run_build(config_path)
    assert result.returncode == 0, f"Build failed for {config_name}"
    
    # Verify output directory
    gen_path = ROOT / expected["out"]
    assert gen_path.exists(), f"Output directory {gen_path} not created"
    
    # Verify generated files
    verify_basic_structure(gen_path, expected["has_init"])
    verify_type_stubs(gen_path, expected["has_mypy"], expected["has_mypy_grpc"])
    verify_pyright_header(gen_path, expected["has_pyright_header"])
    verify_relative_import_rewrite(gen_path)


def test_build_and_verify():
    """Original test with default pyproject.toml for backward compatibility"""
    # build
    cfg = ROOT / "pyproject.toml"
    assert cfg.exists()
    result = subprocess.run(
        ["uv", "run", str(BIN), "build", "--pyproject", str(cfg)],
        cwd=ROOT,
        capture_output=True,
        text=True,
    )
    print(result.stdout)
    print(result.stderr)
    assert result.returncode == 0

    # verify generated files structure
    gen = ROOT / "src" / "proto_importer_e2e" / "generated"

    # Payment service files
    assert (gen / "proto" / "payment" / "payment_pb2.py").exists()
    assert (gen / "proto" / "payment" / "payment_pb2_grpc.py").exists()
    assert (gen / "proto" / "payment" / "types_pb2.py").exists()
    assert (gen / "proto" / "payment" / "types_pb2_grpc.py").exists()
    assert (gen / "proto" / "payment" / "__init__.py").exists()

    # User service files
    assert (gen / "proto" / "user" / "user_pb2.py").exists()
    assert (gen / "proto" / "user" / "user_pb2_grpc.py").exists()
    assert (gen / "proto" / "user" / "types_pb2.py").exists()
    assert (gen / "proto" / "user" / "types_pb2_grpc.py").exists()
    assert (gen / "proto" / "user" / "__init__.py").exists()

    # Inventory service files
    assert (gen / "proto" / "inventory" / "inventory_pb2.py").exists()
    assert (gen / "proto" / "inventory" / "inventory_pb2_grpc.py").exists()
    assert (gen / "proto" / "inventory" / "__init__.py").exists()

    # Root init files
    assert (gen / "proto" / "__init__.py").exists()
    assert (gen / "__init__.py").exists()

    # relative import rewrite example: payment_pb2 should import types_pb2 relatively
    payment_py = (gen / "proto" / "payment" / "payment_pb2.py").read_text(encoding="utf-8")
    assert "from . import types_pb2" in payment_py
    assert "import proto.payment.types_pb2" not in payment_py