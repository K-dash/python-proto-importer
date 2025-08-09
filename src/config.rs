use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy)]
pub enum Backend {
    Protoc,
    Buf,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct AppConfig {
    pub backend: Backend,
    pub python_exe: String,
    pub include: Vec<PathBuf>,
    pub inputs: Vec<String>,
    pub out: PathBuf,
    pub generate_mypy: bool,
    pub generate_mypy_grpc: bool,
    pub postprocess: PostProcess,
    pub verify: Option<Verify>,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct PostProcess {
    pub protoletariat: bool,
    pub fix_pyi: bool,
    pub create_package: bool,
    pub exclude_google: bool,
    pub pyright_header: bool,
    pub module_suffixes: Vec<String>,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct Verify {
    pub mypy_cmd: Option<Vec<String>>,
    pub pyright_cmd: Option<Vec<String>>,
}

// --- Raw TOML structures ---
#[derive(Deserialize)]
struct PyProject {
    tool: Option<ToolSection>,
}

#[derive(Deserialize)]
struct ToolSection {
    #[serde(rename = "python_proto_importer")]
    python_proto_importer: Option<ImporterRoot>,
}

#[derive(Deserialize)]
struct ImporterRoot {
    #[serde(flatten)]
    core: ImporterCore,
    verify: Option<VerifyToml>,
}

#[allow(dead_code)]
#[derive(Deserialize)]
struct ImporterCore {
    backend: Option<String>,
    python_exe: Option<String>,
    include: Option<Vec<String>>, // paths/globs
    inputs: Option<Vec<String>>,  // globs
    out: Option<String>,
    mypy: Option<bool>,
    mypy_grpc: Option<bool>,
    buf_gen_yaml: Option<String>,
    postprocess: Option<PostProcessToml>,
}

#[allow(dead_code)]
#[derive(Deserialize)]
struct PostProcessToml {
    protoletariat: Option<bool>,
    fix_pyi: Option<bool>,
    create_package: Option<bool>,
    exclude_google: Option<bool>,
    pyright_header: Option<bool>,
    module_suffixes: Option<Vec<String>>,
}

#[allow(dead_code)]
#[derive(Deserialize)]
struct VerifyToml {
    mypy_cmd: Option<Vec<String>>,
    pyright_cmd: Option<Vec<String>>,
}

impl AppConfig {
    pub fn load(pyproject_path: Option<&Path>) -> Result<Self> {
        let path = match pyproject_path {
            Some(p) => p.to_path_buf(),
            None => PathBuf::from("pyproject.toml"),
        };
        let content = fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let root: PyProject = toml::from_str(&content).context("failed to parse pyproject.toml")?;
        let Some(tool) = root.tool else {
            bail!("[tool.python_proto_importer] not found");
        };
        let Some(importer) = tool.python_proto_importer else {
            bail!("[tool.python_proto_importer] not found");
        };

        let backend = match importer
            .core
            .backend
            .as_deref()
            .unwrap_or("protoc")
            .to_lowercase()
            .as_str()
        {
            "protoc" => Backend::Protoc,
            "buf" => Backend::Buf,
            other => bail!("unsupported backend: {}", other),
        };

        let python_exe = importer
            .core
            .python_exe
            .unwrap_or_else(|| "python3".to_string());
        let mut include = importer
            .core
            .include
            .unwrap_or_default()
            .into_iter()
            .map(PathBuf::from)
            .collect::<Vec<_>>();

        // If include is empty, use current directory as default
        if include.is_empty() {
            include.push(PathBuf::from("."));
        }
        let inputs = importer.core.inputs.unwrap_or_default();
        let out = importer
            .core
            .out
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("generated/python"));

        let generate_mypy = importer.core.mypy.unwrap_or(false);
        let generate_mypy_grpc = importer.core.mypy_grpc.unwrap_or(false);

        let pp = importer.core.postprocess.unwrap_or(PostProcessToml {
            protoletariat: Some(true),
            fix_pyi: Some(true),
            create_package: Some(true),
            exclude_google: Some(true),
            pyright_header: Some(false),
            module_suffixes: None,
        });
        let postprocess = PostProcess {
            protoletariat: pp.protoletariat.unwrap_or(true),
            fix_pyi: pp.fix_pyi.unwrap_or(true),
            create_package: pp.create_package.unwrap_or(true),
            exclude_google: pp.exclude_google.unwrap_or(true),
            pyright_header: pp.pyright_header.unwrap_or(false),
            module_suffixes: pp.module_suffixes.unwrap_or_else(|| {
                vec![
                    "_pb2.py".into(),
                    "_pb2.pyi".into(),
                    "_pb2_grpc.py".into(),
                    "_pb2_grpc.pyi".into(),
                ]
            }),
        };

        let verify = importer.verify.map(|v| Verify {
            mypy_cmd: v.mypy_cmd,
            pyright_cmd: v.pyright_cmd,
        });

        Ok(Self {
            backend,
            python_exe,
            include,
            inputs,
            out,
            generate_mypy,
            generate_mypy_grpc,
            postprocess,
            verify,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn load_minimal_config() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("pyproject.toml");
        fs::write(
            &config_path,
            r#"
[tool.python_proto_importer]
inputs = ["proto/**/*.proto"]
"#,
        )
        .unwrap();

        let config = AppConfig::load(Some(&config_path)).unwrap();

        assert!(matches!(config.backend, Backend::Protoc));
        assert_eq!(config.python_exe, "python3");
        assert_eq!(config.include, vec![PathBuf::from(".")]);
        assert_eq!(config.inputs, vec!["proto/**/*.proto"]);
        assert_eq!(config.out, PathBuf::from("generated/python"));
        assert!(!config.generate_mypy);
        assert!(!config.generate_mypy_grpc);
        assert!(config.postprocess.protoletariat);
        assert!(config.postprocess.fix_pyi);
        assert!(config.postprocess.create_package);
        assert!(config.postprocess.exclude_google);
        assert!(!config.postprocess.pyright_header);
        assert_eq!(
            config.postprocess.module_suffixes,
            vec!["_pb2.py", "_pb2.pyi", "_pb2_grpc.py", "_pb2_grpc.pyi"]
        );
        assert!(config.verify.is_none());
    }

    #[test]
    fn load_full_config() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("pyproject.toml");
        fs::write(
            &config_path,
            r#"
[tool.python_proto_importer]
backend = "buf"
python_exe = "uv"
include = ["proto", "common"]
inputs = ["proto/**/*.proto", "common/**/*.proto"]
out = "src/generated"
mypy = true
mypy_grpc = true

[tool.python_proto_importer.postprocess]
protoletariat = false
fix_pyi = false
create_package = false
exclude_google = false
pyright_header = true
module_suffixes = ["_pb2.py", "_grpc.py"]

[tool.python_proto_importer.verify]
mypy_cmd = ["mypy", "--strict"]
pyright_cmd = ["pyright", "generated"]
"#,
        )
        .unwrap();

        let config = AppConfig::load(Some(&config_path)).unwrap();

        assert!(matches!(config.backend, Backend::Buf));
        assert_eq!(config.python_exe, "uv");
        assert_eq!(
            config.include,
            vec![PathBuf::from("proto"), PathBuf::from("common")]
        );
        assert_eq!(config.inputs, vec!["proto/**/*.proto", "common/**/*.proto"]);
        assert_eq!(config.out, PathBuf::from("src/generated"));
        assert!(config.generate_mypy);
        assert!(config.generate_mypy_grpc);
        assert!(!config.postprocess.protoletariat);
        assert!(!config.postprocess.fix_pyi);
        assert!(!config.postprocess.create_package);
        assert!(!config.postprocess.exclude_google);
        assert!(config.postprocess.pyright_header);
        assert_eq!(
            config.postprocess.module_suffixes,
            vec!["_pb2.py", "_grpc.py"]
        );

        let verify = config.verify.unwrap();
        assert_eq!(verify.mypy_cmd.unwrap(), vec!["mypy", "--strict"]);
        assert_eq!(verify.pyright_cmd.unwrap(), vec!["pyright", "generated"]);
    }

    #[test]
    fn load_empty_include_defaults_to_current_dir() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("pyproject.toml");
        fs::write(
            &config_path,
            r#"
[tool.python_proto_importer]
inputs = ["proto/**/*.proto"]
include = []
"#,
        )
        .unwrap();

        let config = AppConfig::load(Some(&config_path)).unwrap();
        assert_eq!(config.include, vec![PathBuf::from(".")]);
    }

    #[test]
    fn backend_case_insensitive() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("pyproject.toml");
        fs::write(
            &config_path,
            r#"
[tool.python_proto_importer]
backend = "PROTOC"
inputs = ["proto/**/*.proto"]
"#,
        )
        .unwrap();

        let config = AppConfig::load(Some(&config_path)).unwrap();
        assert!(matches!(config.backend, Backend::Protoc));
    }

    #[test]
    fn unsupported_backend_fails() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("pyproject.toml");
        fs::write(
            &config_path,
            r#"
[tool.python_proto_importer]
backend = "unsupported"
inputs = ["proto/**/*.proto"]
"#,
        )
        .unwrap();

        let result = AppConfig::load(Some(&config_path));
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("unsupported backend")
        );
    }

    #[test]
    fn missing_config_section_fails() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("pyproject.toml");
        fs::write(
            &config_path,
            r#"
[tool.other_tool]
something = "value"
"#,
        )
        .unwrap();

        let result = AppConfig::load(Some(&config_path));
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("[tool.python_proto_importer] not found")
        );
    }

    #[test]
    fn missing_file_fails() {
        let result = AppConfig::load(Some(&PathBuf::from("nonexistent.toml")));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("failed to read"));
    }

    #[test]
    fn invalid_toml_fails() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("pyproject.toml");
        fs::write(
            &config_path,
            r#"
[tool.python_proto_importer
# Missing closing bracket
inputs = ["proto/**/*.proto"]
"#,
        )
        .unwrap();

        let result = AppConfig::load(Some(&config_path));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("failed to parse"));
    }

    #[test]
    fn load_default_path() {
        let dir = tempdir().unwrap();
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(&dir).unwrap();

        let config_path = dir.path().join("pyproject.toml");
        fs::write(
            &config_path,
            r#"
[tool.python_proto_importer]
inputs = ["proto/**/*.proto"]
"#,
        )
        .unwrap();

        let config = AppConfig::load(None).unwrap();
        assert_eq!(config.inputs, vec!["proto/**/*.proto"]);

        std::env::set_current_dir(&original_dir).unwrap();
    }

    #[test]
    fn verify_section_optional() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("pyproject.toml");
        fs::write(
            &config_path,
            r#"
[tool.python_proto_importer]
inputs = ["proto/**/*.proto"]

[tool.python_proto_importer.verify]
mypy_cmd = ["mypy"]
# pyright_cmd intentionally omitted
"#,
        )
        .unwrap();

        let config = AppConfig::load(Some(&config_path)).unwrap();
        let verify = config.verify.unwrap();
        assert_eq!(verify.mypy_cmd.unwrap(), vec!["mypy"]);
        assert!(verify.pyright_cmd.is_none());
    }
}
