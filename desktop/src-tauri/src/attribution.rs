// stub: replaced by Task 5 at merge
//! Co-author trailers and "Generated with" footers, removed from commit
//! messages and PR text before Kete publishes them.

use std::path::Path;

/// `message` without any co-author line or "Generated with" footer, and
/// whether anything was removed.
pub fn strip_ai_trailers(message: &str) -> (String, bool) {
    let mut changed = false;
    let kept: Vec<&str> = message
        .lines()
        .filter(|l| {
            let t = l.trim_start();
            let drop = t.to_ascii_lowercase().starts_with("co-authored-by:")
                || t.starts_with("\u{1f916} Generated with")
                || t.starts_with("Generated with [");
            changed |= drop;
            !drop
        })
        .collect();
    if !changed {
        return (message.to_string(), false);
    }
    let mut out = kept.join("\n");
    out.truncate(out.trim_end().len());
    out.push('\n');
    (out, true)
}

/// Rewrite the messages of `branch`'s commits not yet on the remote.
/// Returns how many were rewritten.
pub fn rewrite_unpushed(_repo: &Path, _base: &str, _branch: &str) -> anyhow::Result<usize> {
    Ok(0)
}
