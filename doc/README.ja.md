# python-proto-importer

[![Crates.io](https://img.shields.io/crates/v/python-proto-importer.svg)](https://crates.io/crates/python-proto-importer)
[![PyPI](https://img.shields.io/pypi/v/python-proto-importer.svg)](https://pypi.org/project/python-proto-importer/)
[![CI](https://github.com/K-dash/python-proto-importer/actions/workflows/ci.yml/badge.svg)](https://github.com/K-dash/python-proto-importer/actions)

Rust 製の CLI ツールで、Python 向けの gRPC/Protobuf ワークフローを効率化：コード生成、import 文の安定化、型チェックを単一コマンドで実行。PyPI パッケージ（maturin 経由）および Rust crate として配布。

- **バックエンド**: `protoc` (v0.1)、`buf generate` (v0.2 で対応予定)
- **後処理**: 内部 import を相対 import に変換、`__init__.py`を生成
- **型付け**: オプションで`mypy-protobuf` / `mypy-grpc`による型スタブ生成
- **検証**: import ドライラン、オプションで mypy/pyright 実行

## 目次

- [クイックスタート](#クイックスタート)
- [コマンド](#コマンド)
- [設定](#設定)
  - [コア設定](#コア設定)
  - [後処理設定](#後処理設定)
  - [検証設定](#検証設定)
- [設定例](#設定例)
- [高度な使い方](#高度な使い方)
- [制限事項](#制限事項)
- [コントリビューション](#コントリビューション)
- [ライセンス](#ライセンス)

## クイックスタート

```bash
pip install python-proto-importer
# または
cargo install python-proto-importer
```

`pyproject.toml`に設定を記述：

```toml
[tool.python_proto_importer]
backend = "protoc"
python_exe = "python3"
include = ["proto"]
inputs = ["proto/**/*.proto"]
out = "generated/python"
```

ビルドを実行：

```bash
proto-importer build
```

## コマンド

### `proto-importer doctor`

必要なツール（protoc、buf、grpc_tools、mypy、pyright）の環境チェック。

### `proto-importer build [--pyproject PATH]`

proto ファイルから Python コードを生成し、後処理を適用、検証を実行。

オプション：

- `--pyproject PATH`: pyproject.toml のパス（デフォルト: `./pyproject.toml`）
- `--no-verify`: 生成後の検証をスキップ
- `--postprocess-only`: 生成をスキップし、後処理のみ実行（実験的）

### `proto-importer check [--pyproject PATH]`

生成なしで検証のみ実行（import ドライランと型チェック）。

### `proto-importer clean [--pyproject PATH] --yes`

生成された出力ディレクトリを削除。`--yes`による確認が必要。

## 設定

すべての設定は`pyproject.toml`の`[tool.python_proto_importer]`セクションで行います。

### コア設定

| オプション   | 型      | デフォルト           | 説明                                                                                                                                                     |
| ------------ | ------- | -------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `backend`    | string  | `"protoc"`           | コード生成バックエンド。現在は`"protoc"`のみサポート。`"buf"`は v0.2 で対応予定。                                                                        |
| `python_exe` | string  | `"python3"`          | 生成と検証に使用する Python 実行ファイル。`"python3"`、`"python"`、`"uv"`（完全テスト済み）、または`".venv/bin/python"`のようなパス。                    |
| `include`    | array   | `["."]`              | Proto インポートパス（protoc の`--proto_path`として渡される）。空配列の場合は`["."]`がデフォルト。詳細は[Include パスの動作](#includeパスの動作)を参照。 |
| `inputs`     | array   | `[]`                 | 生成対象の proto ファイルの Glob パターン。例：`["proto/**/*.proto"]`。ファイルは`include`パスでフィルタリングされる。                                   |
| `out`        | string  | `"generated/python"` | 生成された Python ファイルの出力ディレクトリ。                                                                                                           |
| `mypy`       | boolean | `false`              | `mypy-protobuf`を使用して mypy 型スタブ（`.pyi`ファイル）を生成。                                                                                        |
| `mypy_grpc`  | boolean | `false`              | `mypy-grpc`を使用して gRPC mypy 型スタブ（`_grpc.pyi`ファイル）を生成。                                                                                  |

#### Include パスの動作

`include`オプションは proto インポートパスを制御し、`inputs`との重要な相互作用があります：

1. **デフォルト動作**: `include`が空または未指定の場合、`["."]`（カレントディレクトリ）がデフォルトになります。

2. **パス解決**: `include`の各パスは protoc に`--proto_path`として渡されます。proto ファイルはこれらのパス内の他の proto のみをインポートできます。

3. **入力フィルタリング**: `inputs`の Glob にマッチしたファイルは、`include`パス配下のもののみに自動的にフィルタリングされます。これにより、Glob が include パス外のファイルにマッチした場合の protoc エラーを防ぎます。

4. **出力構造**: 生成されたファイルは`include`パスからの相対ディレクトリ構造を維持します。例：

   - `include = ["proto"]`で`proto/service/api.proto`のファイルの場合
   - 出力は`{out}/service/api_pb2.py`になります

5. **複数の Include パス**: `include = ["proto/common", "proto/services"]`のような複数パスを指定する場合、同じ相対パスのファイルが競合を引き起こす可能性があることに注意してください。

**例：**

```toml
# シンプルなケース - proto/ディレクトリ下のすべてのproto
include = ["proto"]
inputs = ["proto/**/*.proto"]

# 複数のincludeパス - 別々のprotoルートに便利
include = ["common/proto", "services/proto"]
inputs = ["**/*.proto"]

# 選択的な生成 - 特定のサービスのみ
include = ["."]  # パスの競合を避けるためカレントディレクトリを使用
inputs = ["proto/payment/**/*.proto", "proto/user/**/*.proto"]

# 代替proto構造
include = ["api/definitions"]
inputs = ["api/definitions/**/*.proto"]
```

### 後処理設定

`postprocess`テーブルは生成後の変換を制御します：

| オプション         | 型      | デフォルト | 説明                                                                                                    |
| ------------------ | ------- | ---------- | ------------------------------------------------------------------------------------------------------- |
| `relative_imports` | boolean | `true`     | 生成されたファイル内の絶対 import を相対 import に変換。                                                |
| `fix_pyi`          | boolean | `true`     | `.pyi`ファイル内の型アノテーションを修正（現在は将来の使用のために予約）。                              |
| `create_package`   | boolean | `true`     | すべてのディレクトリに`__init__.py`ファイルを作成。名前空間パッケージ（PEP 420）の場合は`false`に設定。 |
| `exclude_google`   | boolean | `true`     | `google.protobuf`の import を相対 import 変換から除外。                                                 |
| `pyright_header`   | boolean | `false`    | 生成された`_pb2.py`と`_pb2_grpc.py`ファイルに Pyright 抑制ヘッダーを追加。                              |
| `module_suffixes`  | array   | 下記参照   | 後処理中に処理するファイルサフィックス。                                                                |

デフォルトの`module_suffixes`：

```toml
module_suffixes = ["_pb2.py", "_pb2.pyi", "_pb2_grpc.py", "_pb2_grpc.pyi"]
```

### 検証設定

`[tool.python_proto_importer.verify]`セクションはオプションの検証コマンドを設定します：

| オプション    | 型    | デフォルト | 説明                                                                       |
| ------------- | ----- | ---------- | -------------------------------------------------------------------------- |
| `mypy_cmd`    | array | `null`     | mypy 型チェックを実行するコマンド。例：`["mypy", "--strict", "generated"]` |
| `pyright_cmd` | array | `null`     | pyright 型チェックを実行するコマンド。例：`["pyright", "generated"]`       |

**重要な注意事項：**

1. **Import ドライラン**: 常に自動的に実行されます。ツールは生成されたすべての Python モジュールをインポートして有効性を確認します。

2. **型チェック**: 設定されている場合のみ実行されます。ツール（mypy/pyright）は環境内で利用可能である必要があります。

3. **コマンド配列**: コマンドは配列として指定され、最初の要素が実行ファイル、残りの要素が引数になります。

**例：**

```toml
[tool.python_proto_importer.verify]
# uvを使用して型チェッカーを実行
mypy_cmd = ["uv", "run", "mypy", "--strict", "generated/python"]
pyright_cmd = ["uv", "run", "pyright", "generated/python"]

# 直接実行
mypy_cmd = ["mypy", "--config-file", "mypy.ini", "generated"]

# pyrightで.pyiファイルのみをチェック
pyright_cmd = ["pyright", "generated/**/*.pyi"]

# mypyの厳格なチェックから生成されたgRPCファイルを除外
mypy_cmd = ["mypy", "--strict", "--exclude", ".*_grpc\\.py$", "generated"]
```

## 設定例

### 最小設定

```toml
[tool.python_proto_importer]
backend = "protoc"
inputs = ["proto/**/*.proto"]
out = "generated"
```

### フル機能設定

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

### 名前空間パッケージ設定（PEP 420）

```toml
[tool.python_proto_importer]
backend = "protoc"
include = ["proto"]
inputs = ["proto/**/*.proto"]
out = "generated"

[tool.python_proto_importer.postprocess]
create_package = false  # __init__.pyファイルを作成しない
```

### 選択的サービス生成

```toml
[tool.python_proto_importer]
backend = "protoc"
include = ["."]
# 特定のサービスのみ生成
inputs = [
    "proto/authentication/**/*.proto",
    "proto/user_management/**/*.proto"
]
out = "services/generated"
```

### カスタムディレクトリ構造

```toml
[tool.python_proto_importer]
backend = "protoc"
# 非標準のproto配置用
include = ["api/v1/definitions"]
inputs = ["api/v1/definitions/**/*.proto"]
out = "build/python/api"
```

## 高度な使い方

### uv との連携

[uv](https://github.com/astral-sh/uv)は pip と virtualenv を置き換える高速な Python パッケージマネージャーです：

```toml
[tool.python_proto_importer]
python_exe = "uv"  # または uv venvを使用している場合は ".venv/bin/python"
# ... 残りの設定

[tool.python_proto_importer.verify]
mypy_cmd = ["uv", "run", "mypy", "--strict", "generated"]
```

### CI/CD 統合

```yaml
# GitHub Actionsの例
- name: 依存関係のインストール
  run: |
    pip install python-proto-importer
    pip install grpcio-tools mypy-protobuf

- name: protoからPythonコードを生成
  run: proto-importer build

- name: テストの実行
  run: pytest tests/
```

### `include`と`inputs`の違い

python-proto-importer を設定する際に理解すべき最も重要な概念の一つは、`include`と`inputs`の違いです：

#### 🗂️ `include` - "どこを見るか"（検索パス）

protobuf コンパイラ（protoc）が`.proto`ファイルを**探す場所**を指定します。

#### 📄 `inputs` - "何をコンパイルするか"（対象ファイル）

実際に**コンパイルしたい`.proto`ファイル**を glob パターンで指定します。

#### 🏗️ プロジェクト構造の例

```
my-project/
├── api/
│   ├── user/
│   │   └── user.proto          # このファイルをコンパイルしたい
│   └── order/
│       └── order.proto         # このファイルをコンパイルしたい
├── third_party/
│   └── google/
│       └── protobuf/
│           └── timestamp.proto # 依存関係として参照される
└── generated/                  # 出力先
```

#### ⚙️ 設定例

```toml
[tool.python_proto_importer]
include = ["api", "third_party"]           # 検索パス
inputs = ["api/**/*.proto"]                # コンパイル対象
out = "generated"
```

#### 🔍 動作の流れ

1. **`inputs`**: `api/**/*.proto` → `user.proto`と`order.proto`が見つかる
2. **`include`**: `api`と`third_party`を検索パスとして設定
3. **コンパイル**:
   - `user.proto`をコンパイル時、`import "google/protobuf/timestamp.proto"`があれば
   - `third_party/google/protobuf/timestamp.proto`を自動的に見つけられる

#### 🚫 よくある間違い

**❌ NG パターン:**

```toml
# 間違い：inputsに依存ファイルまで含めてしまう
inputs = ["api/**/*.proto", "third_party/**/*.proto"]  # 依存関係まで生成してしまう
include = ["api"]                                      # 検索パスが足りない
```

**✅ OK パターン:**

```toml
# 正解：必要なファイルのみコンパイル、依存関係は検索パスで解決
inputs = ["api/**/*.proto"]                    # コンパイルしたいもののみ
include = ["api", "third_party"]               # 依存関係を含む全検索パス
```

#### 🎯 まとめ

- **`include`** = コンパイラの「目」（どこを見渡すか）
- **`inputs`** = コンパイラの「手」（何を掴んでコンパイルするか）

依存関係は**コンパイルしない**（`inputs`に含めない）けれど、**検索可能にする**必要がある（`include`に含める）。

このアプローチにより、**必要なファイルだけを生成しつつ、依存関係は適切に解決できる**ようになります。

### 複雑な Proto 依存関係の処理

複数のディレクトリにまたがる複雑な proto 依存関係を扱う場合：

```toml
[tool.python_proto_importer]
# 必要なすべてのprotoルートを含める
include = [
    ".",
    "third_party/proto",
    "vendor/proto"
]
# 競合を避けるため特定のパターンを使用
inputs = [
    "src/proto/**/*.proto",
    "third_party/proto/specific_service/**/*.proto"
]
out = "generated"
```

## 制限事項

- **v0.1 の制限事項**：
  - `protoc`バックエンドのみサポート。`buf generate`サポートは v0.2 で予定。
  - Import 書き換えは一般的な`_pb2(_grpc)?.py[i]`パターンをターゲットとしており、より広いカバレッジはテストとともに段階的に追加。
- **既知の動作**：
  - 同じ名前のファイルを持つ複数の`include`パスを使用する場合、protoc が「shadowing」エラーを報告する可能性があります。これを回避するには選択的な`inputs`パターンを使用してください。
  - 生成されたファイル構造は protoc の規約に従います：ファイルは`--proto_path`からの相対位置に配置されます。
  - 型チェッカー（mypy/pyright）は別途インストールされ、PATH または Python 環境で利用可能である必要があります。

## コントリビューション

開発セットアップとガイドラインについては[CONTRIBUTING.md](../CONTRIBUTING.md)を参照してください。

## ライセンス

Apache-2.0。詳細は LICENSE ファイルを参照してください。
