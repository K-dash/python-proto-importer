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

    let re_import = Regex::new(
        r"^(?P<indent>\s*)import\s+(?P<mod>[A-Za-z0-9_\.]+)\s+as\s+(?P<alias>[A-Za-z0-9_]+)\s*$",
    )
    .unwrap();
    let re_from = Regex::new(r"^(?P<indent>\s*)from\s+(?P<pkg>[A-Za-z0-9_\.]+)\s+import\s+(?P<name>[A-Za-z0-9_]+)(?:\s+as\s+(?P<alias>[A-Za-z0-9_]+))?\s*$").unwrap();
    let re_import_simple =
        Regex::new(r"^(?P<indent>\s*)import\s+(?P<mod>[A-Za-z0-9_\.]+)\s*$").unwrap();

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
            let target = path_from_module(root, "", module);
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
                let new_line = format!("{indent}from {from_pkg} import {module}");
                out.push_str(&new_line);
                out.push('\n');
                changed = true;
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
            let target = path_from_module(root, "", module);
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
                let new_line = format!("{indent}from {from_pkg} import {module} as {alias}");
                out.push_str(&new_line);
                out.push('\n');
                changed = true;
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
                continue;
            }
        }
        out.push_str(line);
        out.push('\n');
    }

    Ok((out, changed))
}

#[allow(dead_code)]
pub fn apply_rewrites_in_tree(
    root: &Path,
    exclude_google: bool,
    module_suffixes: &[String],
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
