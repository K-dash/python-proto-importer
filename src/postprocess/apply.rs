use anyhow::Result;
use prost_reflect::DescriptorPool;
use std::path::Path;

/// Placeholder for future rewriting application.
#[allow(dead_code)]
pub fn apply_rewrites(_pool: &DescriptorPool, _root: &Path) -> Result<()> {
    // TODO: use pool + rel_imports to compute and apply in-place edits.
    Ok(())
}
