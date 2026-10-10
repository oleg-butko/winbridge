use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

fn git(root: &Path, args: &[&str]) -> Option<String> {
    Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
}

fn git_identity(root: &Path) -> Option<(String, bool)> {
    // Git searches ancestors. An imported generation must never inherit the
    // identity of a checkout containing its workspace/cache.
    let top = git(root, &["rev-parse", "--show-toplevel"])?;
    if fs::canonicalize(root).ok()? != fs::canonicalize(top).ok()? {
        return None;
    }
    let commit = git(root, &["rev-parse", "HEAD"])?;
    let dirty = git(root, &["status", "--porcelain"]).is_none_or(|s| !s.is_empty());
    Some((commit, dirty))
}

fn git_files(root: &Path) -> Option<Vec<String>> {
    let output = Command::new("git")
        .args(["ls-files", "-z"])
        .current_dir(root)
        .output()
        .ok()
        .filter(|output| output.status.success())?;
    Some(
        String::from_utf8(output.stdout)
            .ok()?
            .split('\0')
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
            .collect(),
    )
}

fn main() {
    let root = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("../..");
    let manifest_path = root.join("winbridge-source-manifest.json");
    println!("cargo:rerun-if-changed={}", manifest_path.display());
    let manifest = fs::read(&manifest_path).ok();
    let mut commit = String::new();
    let mut dirty = true;
    let mut hash = String::new();
    if let Some(bytes) = manifest {
        let value: serde_json::Value =
            serde_json::from_slice(&bytes).expect("Invalid WinBridge source manifest");
        commit = value["sourceCommit"].as_str().unwrap_or("").into();
        dirty = value["dirty"].as_bool().unwrap_or(true);
        // Never trust archived identity after edits: compare each build input with its manifest.
        for item in value["files"]
            .as_array()
            .expect("Source manifest files missing")
        {
            let path = item["path"].as_str().expect("Source manifest path missing");
            assert!(
                !path.starts_with('/') && !path.split('/').any(|p| p == ".."),
                "Unsafe source manifest path"
            );
            let input = root.join(path);
            println!("cargo:rerun-if-changed={}", input.display());
            let actual = fs::read(input)
                .map(|data| format!("{:x}", Sha256::digest(data)))
                .ok();
            if actual.as_deref() != item["sha256"].as_str() {
                dirty = true;
            }
        }
        hash = format!("{:x}", Sha256::digest(bytes));
    } else if let Some((head, git_dirty)) = git_identity(&root) {
        commit = head;
        dirty = git_dirty;
        println!(
            "cargo:rerun-if-changed={}",
            root.join(".git/HEAD").display()
        );
        println!(
            "cargo:rerun-if-changed={}",
            root.join(".git/index").display()
        );
        if let Some(names) = git_files(&root) {
            for name in names {
                println!("cargo:rerun-if-changed={}", root.join(name).display());
            }
        } else {
            dirty = true;
        }
        if let Some(reference) = git(&root, &["symbolic-ref", "-q", "HEAD"]) {
            println!(
                "cargo:rerun-if-changed={}",
                root.join(".git").join(reference).display()
            );
        }
    }
    println!("cargo:rustc-env=WINBRIDGE_SOURCE_COMMIT={commit}");
    println!("cargo:rustc-env=WINBRIDGE_SOURCE_DIRTY={dirty}");
    println!("cargo:rustc-env=WINBRIDGE_SOURCE_MANIFEST_SHA256={hash}");
    println!(
        "cargo:rustc-env=WINBRIDGE_BUILD_ID={}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn git_identity_requires_own_checkout_and_preserves_dirty_state() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        for args in [
            vec!["init", "--quiet"],
            vec![
                "-c",
                "user.name=Validation",
                "-c",
                "user.email=validation@local",
                "commit",
                "--allow-empty",
                "--quiet",
                "-m",
                "identity",
            ],
        ] {
            assert!(Command::new("git")
                .args(args)
                .current_dir(root)
                .status()
                .unwrap()
                .success());
        }
        let (commit, dirty) = git_identity(root).unwrap();
        assert_eq!(commit, git(root, &["rev-parse", "HEAD"]).unwrap());
        assert!(!dirty);
        let unicode = " Привет ✓.txt";
        fs::write(root.join(unicode), "build input").unwrap();
        assert!(Command::new("git")
            .args(["add", "--", unicode])
            .current_dir(root)
            .status()
            .unwrap()
            .success());
        assert_eq!(git_files(root), Some(vec![unicode.into()]));
        let generation = root.join("workspaces/project/snapshots/s-example");
        fs::create_dir_all(&generation).unwrap();
        // Plain `git rev-parse HEAD` succeeds here but describes the parent.
        assert_eq!(git(&generation, &["rev-parse", "HEAD"]), Some(commit));
        assert_eq!(git_identity(&generation), None);
        fs::write(generation.join("source.txt"), "untracked source").unwrap();
        assert!(git_identity(root).unwrap().1);
        let unversioned = tempfile::tempdir().unwrap();
        assert_eq!(git_identity(unversioned.path()), None);
    }
}
