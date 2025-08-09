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
        ROOT / "deeply",
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


def run_build(config_path: Path, no_verify: bool = False) -> subprocess.CompletedProcess:
    """Run the build command with given config"""
    cmd = ["uv", "run", str(BIN), "build", "--pyproject", str(config_path)]
    if no_verify:
        cmd.append("--no-verify")
        
    result = subprocess.run(
        cmd,
        cwd=ROOT,
        capture_output=True,
        text=True,
    )
    print(result.stdout)
    print(result.stderr)
    return result


def verify_partial_structure(gen_path: Path, expected_services: list, has_init: bool = True):
    """Verify only expected services are generated"""
    # Check expected services exist
    for service in expected_services:
        service_path = gen_path / "proto" / service
        assert service_path.exists(), f"Expected service {service} not found at {service_path}"
        
        # Check for at least one .py file in the service directory
        py_files = list(service_path.glob("*.py"))
        py_files = [f for f in py_files if f.name != "__init__.py"]
        assert len(py_files) > 0, f"No .py files found in {service_path}"
    
    # Check unexpected services don't exist
    all_services = ["payment", "user", "inventory"]
    for service in all_services:
        if service not in expected_services:
            service_path = gen_path / "proto" / service
            assert not service_path.exists(), f"Unexpected service {service} found at {service_path}"
    
    # Check __init__.py files
    if has_init:
        for service in expected_services:
            init_file = gen_path / "proto" / service / "__init__.py"
            assert init_file.exists(), f"Missing __init__.py in {gen_path / 'proto' / service}"


def verify_alt_structure(gen_path: Path, has_init: bool = True):
    """Verify alternative proto structure"""
    # Note: protoc generates files relative to the --proto_path
    # Since we use "proto_alt/services" as proto_path, files are generated
    # directly under core/ and api/, not under proto_alt/services/
    
    # Check core service
    assert (gen_path / "core" / "health_pb2.py").exists()
    assert (gen_path / "core" / "health_pb2_grpc.py").exists()
    
    # Check api service
    assert (gen_path / "api" / "status_pb2.py").exists()
    assert (gen_path / "api" / "status_pb2_grpc.py").exists()
    
    # Check __init__.py files
    if has_init:
        assert (gen_path / "__init__.py").exists()
        assert (gen_path / "core" / "__init__.py").exists()
        assert (gen_path / "api" / "__init__.py").exists()


def verify_nested_output(gen_path: Path):
    """Verify deeply nested output directory structure"""
    # Check that the nested path exists
    assert gen_path.exists(), f"Nested output path {gen_path} not created"
    
    # Check all parent directories were created
    assert gen_path.parent.exists()
    assert gen_path.parent.parent.exists()
    assert gen_path.parent.parent.parent.exists()
    
    # Check generated files exist in the nested location
    assert (gen_path / "proto" / "payment" / "payment_pb2.py").exists()
    assert (gen_path / "proto" / "user" / "user_pb2.py").exists()
    assert (gen_path / "proto" / "inventory" / "inventory_pb2.py").exists()


def verify_src_structure(gen_path: Path, has_init: bool = True):
    """Verify src/ directory structure with order and billing services"""
    # Check order service
    assert (gen_path / "order" / "order_pb2.py").exists()
    assert (gen_path / "order" / "order_pb2_grpc.py").exists()
    
    # Check billing service 
    assert (gen_path / "billing" / "billing_pb2.py").exists()
    assert (gen_path / "billing" / "billing_pb2_grpc.py").exists()
    
    # Check __init__.py files
    if has_init:
        assert (gen_path / "__init__.py").exists()
        assert (gen_path / "order" / "__init__.py").exists()
        assert (gen_path / "billing" / "__init__.py").exists()


@pytest.mark.parametrize("config_name,expected", [
    ("config_different_include", {
        "out": "generated/selective_include",
        "expected_services": ["payment", "user"],  # inventory should not be included
        "has_init": True,
        "verify_func": "partial",
    }),
    
    ("config_nested_out", {
        "out": "deeply/nested/generated/output",
        "expected_services": ["payment", "user", "inventory"],
        "has_init": True,
        "verify_func": "nested",
    }),
    
    ("config_selective_inputs", {
        "out": "generated/selective_inputs",
        "expected_services": ["payment"],  # only payment
        "has_init": True,
        "verify_func": "partial",
    }),
    
    ("config_alt_proto_path", {
        "out": "generated/alt_structure",
        "has_init": True,
        "verify_func": "alt",
    }),
    
    ("config_relative_paths", {
        "out": "generated/relative_paths",
        "expected_services": ["payment", "user", "inventory"],
        "has_init": True,
        "verify_func": "partial",
    }),
    
    ("config_empty_include", {
        "out": "generated/empty_include",
        "expected_services": ["payment", "user", "inventory"],
        "has_init": True,
        "verify_func": "partial",
    }),
    
    ("config_src_structure", {
        "out": "src/proto_importer_e2e/generated",
        "has_init": True,
        "verify_func": "src",
    }),
])
def test_extended_configs(config_name, expected):
    """Test build with extended configuration files"""
    config_path = ROOT / "configs" / f"{config_name}.toml"
    assert config_path.exists(), f"Config file {config_path} not found"
    
    # Run build
    no_verify = expected.get("no_verify", False)
    result = run_build(config_path, no_verify)
    
    # Check if build succeeded or failed as expected
    if result.returncode != 0:
        print(f"\n=== BUILD FAILED for {config_name} ===")
        print(f"Exit code: {result.returncode}")
        print(f"STDOUT:\n{result.stdout}")
        print(f"STDERR:\n{result.stderr}")
        # Report the failure but don't modify the test to pass
        pytest.fail(f"Build failed for {config_name} with exit code {result.returncode}")
    
    # Verify output directory
    gen_path = ROOT / expected["out"]
    assert gen_path.exists(), f"Output directory {gen_path} not created"
    
    # Run appropriate verification
    verify_func = expected.get("verify_func", "partial")
    if verify_func == "partial":
        verify_partial_structure(gen_path, expected["expected_services"], expected["has_init"])
    elif verify_func == "nested":
        verify_nested_output(gen_path)
        verify_partial_structure(gen_path, expected["expected_services"], expected["has_init"])
    elif verify_func == "alt":
        verify_alt_structure(gen_path, expected["has_init"])
    elif verify_func == "src":
        verify_src_structure(gen_path, expected["has_init"])
    else:
        pytest.fail(f"Unknown verify_func: {verify_func}")