import os
import subprocess
from pathlib import Path

HERE = Path(__file__).resolve().parent.parent
ROOT = HERE
BIN = Path("/Users/furukawa/oss/python-proto-impoter/target/release/python-proto-importer")


def test_build_and_verify():
    # build
    cfg = ROOT / "pyproject.proto_importer.toml"
    assert cfg.exists()
    result = subprocess.run(
        [str(BIN), "build", "--pyproject", str(cfg)],
        cwd=ROOT,
        capture_output=True,
        text=True,
    )
    print(result.stdout)
    print(result.stderr)
    assert result.returncode == 0

    # sanity: import dry-run path
    gen = ROOT / "generated"
    assert (gen / "payment" / "v1" / "thing1_pb2.py").exists()
    assert (gen / "payment" / "v1" / "thing1_pb2_grpc.py").exists()
    assert (gen / "payment" / "v1" / "__init__.py").exists()
