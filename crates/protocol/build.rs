use sha2::{Digest, Sha256};
use std::{
    fs,
    path::PathBuf,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

fn main() {
    let root = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("../..");
    let git = |args: &[&str]| {
        Command::new("git")
            .args(args)
            .current_dir(&root)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
    };
    let manifest_path = root.join("winbridge-source-manifest.json");
    println!("cargo:rerun-if-changed={}", manifest_path.display());
    let manifest = fs::read(&manifest_path).ok();
    let mut commit = String::new();
    let mut dirty = false;
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
    } else if let Some(head) = git(&["rev-parse", "HEAD"]) {
        commit = head;
        dirty = git(&["status", "--porcelain"]).is_none_or(|s| !s.is_empty());
        println!(
            "cargo:rerun-if-changed={}",
            root.join(".git/HEAD").display()
        );
        println!(
            "cargo:rerun-if-changed={}",
            root.join(".git/index").display()
        );
        if let Some(names) = git(&["ls-files"]) {
            for name in names.lines() {
                println!("cargo:rerun-if-changed={}", root.join(name).display());
            }
        }
        if let Some(reference) = git(&["symbolic-ref", "-q", "HEAD"]) {
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
