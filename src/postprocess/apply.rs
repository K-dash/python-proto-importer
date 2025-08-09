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
    // Try canonicalize to normalize symlinks and relative segments; fall back to raw paths
    let canonicalize_or =
        |p: &Path| -> PathBuf { std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf()) };
    let from_c = canonicalize_or(from_dir);
    let to_c = canonicalize_or(to_dir);

    let from = from_c.components().collect::<Vec<_>>();
    let to = to_c.components().collect::<Vec<_>>();
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
    let re_from_any =
        Regex::new(r"^(?P<indent>\s*)from\s+(?P<pkg>[A-Za-z0-9_\.]+)\s+import\s+(?P<rest>.*)$")
            .unwrap();
    let re_import_simple =
        Regex::new(r"^(?P<indent>\s*)import\s+(?P<mod>[A-Za-z0-9_\.]+)\s*(?:#.*)?$").unwrap();
    let re_import_list = Regex::new(r"^(?P<indent>\s*)import\s+(?P<rest>.+)$").unwrap();

    // State for collecting parenthesized multi-line 'from ... import (...)' blocks
    let mut pending_from_block: Option<(String, String, String)> = None; // (indent, pkg, collected)

    for line in content.lines() {
        // Handle continuation of a parenthesized from-import block
        if let Some((indent, pkg, mut collected)) = pending_from_block.take() {
            collected.push('\n');
            collected.push_str(line);
            // Check if parentheses are balanced now
            let opens = collected.matches('(').count();
            let closes = collected.matches(')').count();
            if closes < opens {
                // Still pending
                pending_from_block = Some((indent, pkg, collected));
                continue;
            }

            // Process the full block
            let processed = process_from_import_list(
                &indent,
                &pkg,
                &collected,
                file_dir,
                root,
                exclude_google,
            )?;
            out.push_str(&processed.output);
            changed |= processed.changed;
            continue;
        }
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

        // Handle comma-separated 'import a, b as c' by splitting into tokens
        if let Some(caps) = re_import_list.captures(line) {
            let indent = &caps["indent"];
            let rest = &caps["rest"]; // may contain commas and aliases
            if rest.contains(',') {
                let mut any_local_change = false;
                for tok in rest.split(',') {
                    let token = tok.trim();
                    if token.is_empty() {
                        continue;
                    }
                    // token: module[ as alias]
                    let mut parts = token.split_whitespace().collect::<Vec<_>>();
                    if parts.is_empty() {
                        continue;
                    }
                    // reconstruct alias if provided
                    let mut alias: Option<&str> = None;
                    if parts.len() >= 3 && parts[parts.len() - 2] == "as" {
                        alias = Some(parts[parts.len() - 1]);
                        parts.truncate(parts.len() - 2);
                    }
                    let module = parts.join(" ");
                    if (module.ends_with("_pb2") || module.ends_with("_pb2_grpc"))
                        && !(exclude_google && module.starts_with("google.protobuf"))
                    {
                        let (module_path, leaf) = split_module_qualname(&module);
                        let target = path_from_module(root, &module_path, &leaf);
                        if target.exists() {
                            if let Some((ups, remainder)) = compute_relative_import_prefix(
                                file_dir,
                                target.parent().unwrap_or(root),
                            ) {
                                let dots = ".".repeat(ups + 1);
                                let from_pkg = if remainder.is_empty() {
                                    dots
                                } else {
                                    format!("{dots}{remainder}")
                                };
                                if let Some(a) = alias {
                                    out.push_str(&format!(
                                        "{indent}from {from_pkg} import {leaf} as {a}\n"
                                    ));
                                } else {
                                    out.push_str(&format!(
                                        "{indent}from {from_pkg} import {leaf}\n"
                                    ));
                                }
                                changed = true;
                                any_local_change = true;
                                continue;
                            }
                        }
                    }
                    // Fallback: keep original token as a separate import line
                    out.push_str(&format!("{indent}import {token}\n"));
                }
                if any_local_change {
                    continue;
                }
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
        // Handle single-name 'from pkg import name [as alias]'
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
                // ups=0 -> same level (use "." + remainder)
                // ups=1 -> parent level (use ".." + remainder)
                let dots = if ups == 0 {
                    ".".to_string()
                } else {
                    ".".repeat(ups + 1)
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

        // Handle 'from pkg import a, b as c' (single-line) or start of parenthesized block
        if let Some(caps) = re_from_any.captures(line) {
            let indent = caps["indent"].to_string();
            let pkg = caps["pkg"].to_string();
            let rest = caps["rest"].trim();
            if rest.starts_with('(') && !rest.contains(')') {
                // Begin collecting a multi-line parenthesized block
                pending_from_block = Some((indent, pkg, line.to_string()));
                continue;
            }
            if rest.contains(',') || rest.starts_with('(') {
                // Process possibly parenthesized single-line list
                let processed =
                    process_from_import_list(&indent, &pkg, line, file_dir, root, exclude_google)?;
                out.push_str(&processed.output);
                changed |= processed.changed;
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

struct FromImportProcessResult {
    output: String,
    changed: bool,
}

fn process_from_import_list(
    indent: &str,
    pkg: &str,
    full_line_or_block: &str,
    file_dir: &Path,
    root: &Path,
    exclude_google: bool,
) -> Result<FromImportProcessResult> {
    // Extract everything after 'from <pkg> import'
    let after_import = full_line_or_block
        .split_once(" import ")
        .map(|(_, s)| s.trim())
        .unwrap_or_else(|| full_line_or_block.trim());

    // Remove wrapping parentheses and trailing comment lines
    let mut inner = after_import.trim();
    if inner.starts_with('(') {
        // Remove the first '(' and the matching last ')'
        // For robustness, just trim leading '(' and trailing ')' and whitespace/commas
        inner = inner.trim_start_matches('(');
        inner = inner.trim_end();
        if inner.ends_with(')') {
            inner = &inner[..inner.len() - 1];
        }
    }

    // Split by commas across potential multi-lines
    let mut tokens: Vec<String> = Vec::new();
    for raw in inner.lines() {
        let no_comment = match raw.find('#') {
            Some(idx) => &raw[..idx],
            None => raw,
        };
        for part in no_comment.split(',') {
            let t = part.trim();
            if !t.is_empty() {
                tokens.push(t.to_string());
            }
        }
    }

    if tokens.is_empty() {
        return Ok(FromImportProcessResult {
            output: format!("{}from {} import {}\n", indent, pkg, inner.trim()),
            changed: false,
        });
    }

    // Partition into rewritable and others
    let mut rewrite_items: Vec<(String, Option<String>)> = Vec::new();
    let mut keep_items: Vec<String> = Vec::new();
    for tok in tokens {
        // token: name [as alias]
        let mut name = tok.as_str();
        let mut alias: Option<String> = None;
        if let Some(pos) = tok.rfind(" as ") {
            name = tok[..pos].trim();
            alias = Some(tok[pos + 4..].trim().to_string());
        }
        if (name.ends_with("_pb2") || name.ends_with("_pb2_grpc"))
            && !(exclude_google && pkg.starts_with("google.protobuf"))
        {
            // Check target exists
            let target = path_from_module(root, pkg, name);
            if target.exists() {
                rewrite_items.push((name.to_string(), alias));
                continue;
            }
        }
        keep_items.push(tok);
    }

    if rewrite_items.is_empty() {
        // No change
        return Ok(FromImportProcessResult {
            output: format!("{}{}\n", indent, full_line_or_block.trim()),
            changed: false,
        });
    }

    // Compute relative from-pkg using any one item's target (they share pkg)
    let any_name = &rewrite_items[0].0;
    let target = path_from_module(root, pkg, any_name);
    let (ups, remainder) =
        compute_relative_import_prefix(file_dir, target.parent().unwrap_or(root))
            .unwrap_or((0, String::new()));
    let dots = if ups == 0 {
        ".".to_string()
    } else {
        ".".repeat(ups + 1)
    };
    let from_pkg = if remainder.is_empty() {
        dots
    } else {
        format!("{dots}{remainder}")
    };

    // Build output lines: first the rewritten relative import
    let mut output = String::new();
    let list = rewrite_items
        .into_iter()
        .map(|(n, a)| match a {
            Some(x) => format!("{} as {}", n, x),
            None => n,
        })
        .collect::<Vec<_>>()
        .join(", ");
    output.push_str(&format!("{}from {} import {}\n", indent, from_pkg, list));

    // Keep the remaining items via original absolute import if any
    if !keep_items.is_empty() {
        let keep_list = keep_items.join(", ");
        output.push_str(&format!("{}from {} import {}\n", indent, pkg, keep_list));
    }

    Ok(FromImportProcessResult {
        output,
        changed: true,
    })
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
            // Pre-filter: if allowed_basenames (from FDS) are provided,
            // skip files that don't contain any target basename
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
    fn compute_prefix_same_level() {
        // Test sibling directories: billing/ and order/ under generated/
        let from = Path::new("generated/billing");
        let to = Path::new("generated/order");
        let (ups, rem) = compute_relative_import_prefix(from, to).unwrap();
        assert_eq!(ups, 1); // Go up one level to parent, then down to sibling
        assert_eq!(rem, "order");
    }

    #[test]
    fn compute_prefix_with_relative_segments() {
        // from: ./a/./b, to: a/c/../c/d -> expect up 1 and remainder c.d
        let tmp = tempdir().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join("a/b")).unwrap();
        std::fs::create_dir_all(root.join("a/c/d")).unwrap();

        let from = root.join("./a/./b");
        let to = root.join("a/c/../c/d");
        let (ups, rem) = compute_relative_import_prefix(&from, &to).unwrap();
        assert_eq!(ups, 1);
        assert_eq!(rem, "c.d");
    }

    #[cfg(unix)]
    #[test]
    fn compute_prefix_with_symlink() {
        use std::os::unix::fs::symlink;
        let tmp = tempdir().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join("real/order")).unwrap();
        std::fs::create_dir_all(root.join("real/billing")).unwrap();
        // symlink 'gen' -> 'real'
        symlink(root.join("real"), root.join("gen")).unwrap();

        let from = root.join("gen/billing");
        let to = root.join("real/order");
        let (ups, rem) = compute_relative_import_prefix(&from, &to).unwrap();
        // After canonicalize, common prefix is root/real, expect up 1 and remainder order
        assert_eq!(ups, 1);
        assert_eq!(rem, "order");
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

    #[test]
    fn rewrite_from_multi_items_single_line() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        // structure: pkg/a_pb2.py and pkg/b_pb2_grpc.py
        std::fs::create_dir_all(root.join("pkg")).unwrap();
        fs::write(root.join("pkg/a_pb2.py"), "# a").unwrap();
        fs::write(root.join("pkg/b_pb2_grpc.py"), "# b").unwrap();
        let file_dir = root.join("pkg");
        let content = "from pkg import a_pb2, b_pb2_grpc as bgrpc\n";
        let (out, changed) = rewrite_lines_in_content(content, &file_dir, root, false).unwrap();
        assert!(changed);
        assert_eq!(out.trim_end(), "from . import a_pb2, b_pb2_grpc as bgrpc");
    }

    #[test]
    fn rewrite_from_parenthesized_multi_line() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("pkg")).unwrap();
        fs::write(root.join("pkg/a_pb2.py"), "# a").unwrap();
        fs::write(root.join("pkg/b_pb2.py"), "# b").unwrap();
        let file_dir = root.join("pkg");
        let content = "from pkg import (\n    a_pb2,\n    b_pb2 as bb,\n)\n";
        let (out, changed) = rewrite_lines_in_content(content, &file_dir, root, false).unwrap();
        assert!(changed);
        assert_eq!(out.trim_end(), "from . import a_pb2, b_pb2 as bb");
    }

    #[test]
    fn rewrite_import_list_into_multiple_lines() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("pkg/sub")).unwrap();
        fs::write(root.join("pkg/a_pb2.py"), "# a").unwrap();
        fs::write(root.join("pkg/sub/b_pb2.py"), "# b").unwrap();
        let file_dir = root; // importing at project root
        let content = "import pkg.a_pb2, pkg.sub.b_pb2 as bb, json\n";
        let (out, changed) = rewrite_lines_in_content(content, file_dir, root, false).unwrap();
        assert!(changed);
        // Should produce two from-import lines and keep 'json' as import
        let lines: Vec<_> = out.lines().collect();
        assert_eq!(lines.len(), 3);
        assert!(
            lines[0].starts_with("from .pkg import a_pb2")
                || lines[1].starts_with("from .pkg import a_pb2")
        );
        assert!(
            lines
                .iter()
                .any(|l| l.starts_with("from .pkg.sub import b_pb2 as bb"))
        );
        assert!(lines.iter().any(|l| *l == "import json"));
    }

    #[test]
    fn keep_google_protobuf_in_multi() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("pkg")).unwrap();
        fs::write(root.join("pkg/a_pb2.py"), "# a").unwrap();
        let file_dir = root.join("pkg");
        let content = "from google.protobuf import timestamp_pb2, duration_pb2\nfrom pkg import a_pb2, timestamp_pb2\n";
        let (out, changed) = rewrite_lines_in_content(content, &file_dir, root, true).unwrap();
        assert!(changed); // a_pb2 should change but google protobuf kept
        assert!(out.contains("from . import a_pb2"));
        assert!(out.contains("from google.protobuf import timestamp_pb2, duration_pb2"));
    }

    #[test]
    fn rewrite_from_sibling_directory() {
        // Test the actual scenario: billing/ importing from order/
        let dir = tempdir().unwrap();
        let root = dir.path();

        // Create structure: generated/billing/ and generated/order/
        fs::create_dir_all(root.join("billing")).unwrap();
        fs::create_dir_all(root.join("order")).unwrap();
        fs::write(root.join("order/order_pb2.py"), "# order module\n").unwrap();

        let billing_content = "from order import order_pb2 as order_dot_order__pb2\n";
        fs::write(root.join("billing/billing_pb2.py"), billing_content).unwrap();

        let modified = apply_rewrites_in_tree(root, false, &["_pb2.py".into()], None).unwrap();
        assert_eq!(modified, 1);

        let billing = fs::read_to_string(root.join("billing/billing_pb2.py")).unwrap();
        // Should be sibling import: from ..order import order_pb2 (up one level, then down)
        assert_eq!(
            billing,
            "from ..order import order_pb2 as order_dot_order__pb2\n"
        );
    }
}
