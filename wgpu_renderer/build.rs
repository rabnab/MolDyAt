//! Bakes the git commit into the binary (`GIT_COMMIT`), so perf reports name
//! the code they measured (`--timings`).
//!
//! `<short hash>`, with `-dirty` if tracked files differ from HEAD, or
//! `unknown` without git. Reruns when the compiled sources change (so the
//! dirty flag sees unstaged edits too) and when HEAD, the branch ref, the
//! packed refs or the index change. Git paths come from `git rev-parse
//! --git-path`, which also resolves linked worktrees, and only paths that
//! exist are watched: Cargo treats a missing one as always stale.
use std::path::{Path, PathBuf};
use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn watch(path: impl AsRef<Path>) {
    let path = path.as_ref();
    if path.exists() {
        println!("cargo:rerun-if-changed={}", path.display());
    }
}

fn git_path(name: &str) -> Option<PathBuf> {
    git(&["rev-parse", "--git-path", name]).map(PathBuf::from)
}

fn main() {
    // The sources this binary is built from (the workspace's two crates).
    for source in [
        "src",
        "Cargo.toml",
        "../core/src",
        "../core/Cargo.toml",
        "../Cargo.toml",
    ] {
        watch(source);
    }
    for name in ["HEAD", "index", "packed-refs", "logs/HEAD"] {
        if let Some(path) = git_path(name) {
            watch(path);
        }
    }
    if let Some(head_ref) = git(&["symbolic-ref", "-q", "HEAD"])
        && let Some(path) = git_path(&head_ref)
    {
        watch(path);
    }

    let commit = match git(&["rev-parse", "--short", "HEAD"]) {
        Some(hash) => {
            let clean = Command::new("git")
                .args(["diff", "--quiet", "HEAD"])
                .status()
                .is_ok_and(|status| status.success());
            if clean { hash } else { format!("{hash}-dirty") }
        }
        None => "unknown".to_string(),
    };
    println!("cargo:rustc-env=GIT_COMMIT={commit}");
}
