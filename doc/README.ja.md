# python-proto-importer（日本語）

Python の gRPC/Protobuf 開発を、生成 → import 安定化 → 型検証までワンコマンドで実行する Rust 製 CLI です。PyPI 配布（maturin 同梱）および crates.io 配布に対応します。

- **バックエンド**: `protoc`（v0.1）、`buf generate`（v0.2 予定）
- **後処理**: 生成物内部を相対 import に統一、`__init__.py` 自動生成
- **型**: `mypy-protobuf` / `mypy-grpc` の出力（`.pyi`）をオプションで生成
- **検証**: import ドライラン、必要に応じて mypy / pyright 実行

## クイックスタート

```bash
pip install python-proto-importer
# もしくは
cargo install python-proto-importer
```

## 設定例（protoc backend）

```toml
[tool.python_proto_importer]
backend = "protoc"
python_exe = "python3" # あるいは "uv"
include = ["proto"]
inputs = ["proto/**/*.proto"]
out = "generated/python"
# 型スタブの生成（任意）
mypy = true
mypy_grpc = true
postprocess = { protoletariat = true, fix_pyi = true, create_package = true, exclude_google = true }

[tool.python_proto_importer.verify]
# 最小限の例。プロジェクトに合わせて調整してください
mypy_cmd = ["uv", "run", "mypy", "--strict", "generated/python"]
pyright_cmd = ["uv", "run", "pyright", "generated/python"]
```

## Buf backend の設定例（v0.2 予定）

`buf generate` 対応は v0.2 で追加予定です。暫定の設定例は次の通りです。

```toml
[tool.python_proto_importer]
backend = "buf"
buf_gen_yaml = "buf.gen.yaml"
postprocess = { protoletariat = true, fix_pyi = true, create_package = true, exclude_google = true }

[tool.python_proto_importer.verify]
# プロジェクトに応じて調整してください
mypy_cmd = ["uv", "run", "mypy", "--strict", "gen/python"]
```

## コントリビュート

E2E テストの実行手順を含む開発者向けドキュメントは、[CONTRIBUTING.md](../CONTRIBUTING.md) を参照してください。

## 制限事項

- v0.1 は `protoc` バックエンドのみ対応。`buf generate` は v0.2 で対応予定です。
- import 置換は一般的な `_pb2(_grpc)?.py[ i]` パターンを対象としており、網羅性はテストを追加しながら拡張していきます。
- 型検証（mypy/pyright）は設定した場合のみ実行され、各ツールは PATH 上に必要です。
- ネームスペースパッケージ（PEP 420）に対しては、import 成功を優先して `__init__.py` を生成するのがデフォルト。設定で無効化できます。

## ライセンス

Apache-2.0。Protoletariat / fix-protobuf-imports など既存 OSS の挙動を参考に、Rust で独自実装しています。詳細は LICENSE を参照してください。
