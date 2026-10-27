use std::fs::{self, File};
use winbridge_platform::{require_single_link, require_single_link_handle};

#[test]
fn handle_and_path_refuse_the_same_hardlinked_file() {
    println!(
        "FILE_LINKS_BUILD {}",
        serde_json::to_string(&winbridge_protocol::build_identity()).unwrap()
    );
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("original");
    fs::write(&path, b"fixture").unwrap();
    let file = File::open(&path).unwrap();
    require_single_link(&path).unwrap();
    require_single_link_handle(&file).unwrap();
    let alias = temp.path().join("alias");
    fs::hard_link(&path, &alias).unwrap();
    assert!(require_single_link(&path).is_err());
    assert!(require_single_link_handle(&file).is_err());
}

#[cfg(windows)]
#[test]
fn attributes_handle_inspects_exclusive_lease_without_data_access() {
    use std::{fs::OpenOptions, os::windows::fs::OpenOptionsExt};
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("lease");
    let lease = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .share_mode(0)
        .open(&path)
        .unwrap();
    let attributes = OpenOptions::new()
        .read(true)
        .access_mode(0x80)
        .share_mode(7)
        .open(&path)
        .unwrap();
    require_single_link_handle(&attributes).unwrap();
    assert_eq!(
        File::open(&path).unwrap_err().raw_os_error(),
        Some(32),
        "A metadata check must leave the exclusive data lease intact"
    );
    drop(lease);
    File::open(&path).unwrap();
}
