use anyhow::{Context, Result};
#[allow(unused_imports)]
use prost_reflect::DescriptorPool;
use regex::Regex;
use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use walkdir::WalkDir;

fn path_from_module(root: &Path, module_path: &str, leaf: &str) -> PathBuf {
    let mut p = root.to_path_buf();
    if !module_path.is_empty() {
        for part in module_path.split('.') {
            if !part.is_empty() {
                p.push(part);
            }
        }
    }
    p.push(format!("{leaf}.py"));
    p
}

fn split_module_qualname(qualified: &str) -> (String, String) {
    if let Some(idx) = qualified.rfind('.') {
        (
            qualified[..idx].to_string(),
            qualified[idx + 1..].to_string(),
        )
    } else {
        (String::new(), qualified.to_string())
    }
}

fn compute_relative_import_prefix(from_dir: &Path, to_dir: &Path) -> Option<(usize, String)> {
    let from = from_dir.components().collect::<Vec<_>>();
    let to = to_dir.components().collect::<Vec<_>>();
    let mut i = 0usize;
    while i < from.len() && i < to.len() && from[i] == to[i] {
        i += 1;
    }
    let ups = from.len().saturating_sub(i);
    let mut remainder_parts: Vec<String> = Vec::new();
    for comp in &to[i..] {
        if let Component::Normal(os) = comp {
            remainder_parts.push(os.to_string_lossy().to_string());
        }
    }
    Some((
        ups,
        if remainder_parts.is_empty() {
            String::new()
        } else {
            remainder_parts.join(".")
        },
    ))
}

fn rewrite_lines_in_content(
    content: &str,
    file_dir: &Path,
    root: &Path,
    exclude_google: bool,
) -> Result<(String, bool)> {
    let mut changed = false;
    let mut out = String::with_capacity(content.len());
    // map of fully-qualified module -> local name to use in annotations
    let mut module_rewrites: Vec<(String, String)> = Vec::new();

    let re_import = Regex::new(
        r"^(?P<indent>\s*)import\s+(?P<mod>[A-Za-z0-9_\.]+)\s+as\s+(?P<alias>[A-Za-z0-9_]+)\s*(?:#.*)?$",
    )
    .unwrap();
    let re_from = Regex::new(r"^(?P<indent>\s*)from\s+(?P<pkg>[A-Za-z0-9_\.]+)\s+import\s+(?P<name>[A-Za-z0-9_]+)(?:\s+as\s+(?P<alias>[A-Za-z0-9_]+))?\s*(?:#.*)?$").unwrap();
    let re_import_simple =
        Regex::new(r"^(?P<indent>\s*)import\s+(?P<mod>[A-Za-z0-9_\.]+)\s*(?:#.*)?$").unwrap();

    for line in content.lines() {
        if line.trim_start().starts_with("from .") {
            out.push_str(line);
            out.push('\n');
            continue;
        }
        if let Some(caps) = re_import_simple.captures(line) {
            let indent = &caps["indent"];
            let module = &caps["mod"];
            if !module.ends_with("_pb2") && !module.ends_with("_pb2_grpc") {
                out.push_str(line);
                out.push('\n');
                continue;
            }
            if exclude_google && module.starts_with("google.protobuf") {
                out.push_str(line);
                out.push('\n');
                continue;
            }
            let (module_path, leaf) = split_module_qualname(module);
            let target = path_from_module(root, &module_path, &leaf);
            if !target.exists() {
                out.push_str(line);
                out.push('\n');
                continue;
            }
            if let Some((ups, remainder)) =
                compute_relative_import_prefix(file_dir, target.parent().unwrap_or(root))
            {
                // ups=0 -> "." (current), ups=1 -> ".." (parent)
                let dots = ".".repeat(ups + 1);
                let from_pkg = if remainder.is_empty() {
                    dots
                } else {
                    format!("{dots}{remainder}")
                };
                let new_line = format!("{indent}from {from_pkg} import {leaf}");
                out.push_str(&new_line);
                out.push('\n');
                changed = true;
                module_rewrites.push((module.to_string(), leaf.to_string()));
                continue;
            }
        }

        if let Some(caps) = re_import.captures(line) {
            let indent = &caps["indent"];
            let module = &caps["mod"];
            let alias = &caps["alias"];
            if !module.ends_with("_pb2") && !module.ends_with("_pb2_grpc") {
                out.push_str(line);
                out.push('\n');
                continue;
            }
            if exclude_google && module.starts_with("google.protobuf") {
                out.push_str(line);
                out.push('\n');
                continue;
            }
            let (module_path, leaf) = split_module_qualname(module);
            let target = path_from_module(root, &module_path, &leaf);
            if !target.exists() {
                out.push_str(line);
                out.push('\n');
                continue;
            }
            if let Some((ups, remainder)) =
                compute_relative_import_prefix(file_dir, target.parent().unwrap_or(root))
            {
                // ups=0 -> "." (current), ups=1 -> ".." (parent)
                let dots = ".".repeat(ups + 1);
                let from_pkg = if remainder.is_empty() {
                    dots
                } else {
                    format!("{dots}{remainder}")
                };
                let new_line = format!("{indent}from {from_pkg} import {leaf} as {alias}");
                out.push_str(&new_line);
                out.push('\n');
                changed = true;
                module_rewrites.push((module.to_string(), alias.to_string()));
                continue;
            }
        }
        if let Some(caps) = re_from.captures(line) {
            let indent = &caps["indent"];
            let pkg = &caps["pkg"];
            let name = &caps["name"];
            let alias = caps.name("alias").map(|m| m.as_str());
            if !name.ends_with("_pb2") && !name.ends_with("_pb2_grpc") {
                out.push_str(line);
                out.push('\n');
                continue;
            }
            if exclude_google && pkg.starts_with("google.protobuf") {
                out.push_str(line);
                out.push('\n');
                continue;
            }
            let target = path_from_module(root, pkg, name);
            if !target.exists() {
                out.push_str(line);
                out.push('\n');
                continue;
            }
            if let Some((ups, remainder)) =
                compute_relative_import_prefix(file_dir, target.parent().unwrap_or(root))
            {
                let dots = if ups == 0 {
                    ".".to_string()
                } else {
                    ".".repeat(ups)
                };
                let from_pkg = if remainder.is_empty() {
                    dots
                } else {
                    format!("{dots}{remainder}")
                };
                let new_line = if let Some(a) = alias {
                    format!("{indent}from {from_pkg} import {name} as {a}")
                } else {
                    format!("{indent}from {from_pkg} import {name}")
                };
                out.push_str(&new_line);
                out.push('\n');
                changed = true;
                // fully-qualified = pkg.name
                let fq = if pkg.is_empty() {
                    name.to_string()
                } else {
                    format!("{pkg}.{name}")
                };
                let local = alias
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| name.to_string());
                module_rewrites.push((fq, local));
                continue;
            }
        }
        out.push_str(line);
        out.push('\n');
    }
    // After rewriting imports, fix fully-qualified references in annotations
    if !module_rewrites.is_empty() {
        for (from_mod, to_name) in module_rewrites.iter() {
            // replace occurrences like "from_mod.*" to "to_name.*"
            let pattern = regex::Regex::new(&format!(r"\b{}\.", regex::escape(from_mod))).unwrap();
            let replaced = pattern.replace_all(&out, format!("{}.", to_name));
            let new_str = replaced.into_owned();
            if new_str != out {
                changed = true;
                out = new_str;
            }
        }
    }

    Ok((out, changed))
}

#[allow(dead_code)]
pub fn apply_rewrites_in_tree(
    root: &Path,
    exclude_google: bool,
    module_suffixes: &[String],
    allowed_basenames: Option<&std::collections::HashSet<String>>,
) -> Result<usize> {
    let mut modified = 0usize;
    for entry in WalkDir::new(root).into_iter().filter_map(Result::ok) {
        let p = entry.path();
        if p.is_file() {
            let rel = p.strip_prefix(root).unwrap_or(p).to_string_lossy();
            let mut matched = false;
            for s in module_suffixes {
                if (s.ends_with(".py") || s.ends_with(".pyi")) && rel.ends_with(s) {
                    matched = true;
                    break;
                }
            }
            if !matched {
                continue;
            }
            let content = fs::read_to_string(p).with_context(|| format!("read {}", p.display()))?;
            // 事前フィルタ: allowed_basenames（FDS由来）が与えられていれば、
            // 行単位で対象basenameが含まれないファイルはスキップ
            if let Some(allowed) = allowed_basenames {
                if !allowed.iter().any(|b| content.contains(b)) {
                    continue;
                }
            }
            let (new_content, changed) = rewrite_lines_in_content(
                &content,
                p.parent().unwrap_or(root),
                root,
                exclude_google,
            )?;
            if changed {
                let mut f = fs::OpenOptions::new()
                    .write(true)
                    .truncate(true)
                    .open(p)
                    .with_context(|| format!("open {} for write", p.display()))?;
                f.write_all(new_content.as_bytes())
                    .with_context(|| format!("write {}", p.display()))?;
                modified += 1;
            }
        }
    }
    Ok(modified)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn compute_prefix_basic() {
        let _root = Path::new("/");
        let from = Path::new("/a/b");
        let to = Path::new("/a/c/d");
        let (ups, rem) = compute_relative_import_prefix(from, to).unwrap();
        assert_eq!(ups, 1);
        assert_eq!(rem, "c.d");
    }

    #[test]
    fn rewrite_import_alias() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        // target module at root/a_pb2.py
        fs::write(root.join("a_pb2.py"), "# stub").unwrap();
        // file under sub/needs.py
        let sub = root.join("sub");
        fs::create_dir_all(&sub).unwrap();
        let content = "import a_pb2 as a__pb2\n";
        let (out, changed) = rewrite_lines_in_content(content, &sub, root, false).unwrap();
        assert!(changed);
        assert_eq!(out, "from .. import a_pb2 as a__pb2\n");
    }

    #[test]
    fn rewrite_pyi_simple_import() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("a_pb2.py"), "# stub").unwrap();
        let sub = root.join("pkg");
        fs::create_dir_all(&sub).unwrap();
        let content = "import a_pb2\n";
        let (out, changed) = rewrite_lines_in_content(content, &sub, root, false).unwrap();
        assert!(changed);
        assert_eq!(out, "from .. import a_pb2\n");
    }

    #[test]
    fn skip_google_protobuf() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        // no need to create files; should skip due to exclude_google
        let content = "import google.protobuf.timestamp_pb2 as timestamp__pb2\n";
        let (out, changed) = rewrite_lines_in_content(content, root, root, true).unwrap();
        assert!(!changed);
        assert_eq!(out, content);
    }

    #[test]
    fn apply_rewrites_suffix_filter() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        // create structure
        fs::create_dir_all(root.join("x")).unwrap();
        fs::write(root.join("a_pb2.py"), "# a\n").unwrap();
        fs::write(root.join("x/b_pb2.py"), "import a_pb2 as a__pb2\n").unwrap();
        fs::write(root.join("c.py"), "import a_pb2 as a__pb2\n").unwrap();
        let modified = apply_rewrites_in_tree(root, false, &["_pb2.py".into()], None).unwrap();
        // only x/b_pb2.py should be modified
        assert_eq!(modified, 1);
        let b = fs::read_to_string(root.join("x/b_pb2.py")).unwrap();
        assert_eq!(b, "from .. import a_pb2 as a__pb2\n");
        let c = fs::read_to_string(root.join("c.py")).unwrap();
        assert_eq!(c, "import a_pb2 as a__pb2\n");
    }
}
