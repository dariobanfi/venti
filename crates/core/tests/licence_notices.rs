//! The notices the apps carry for the crates compiled into this core,
//! `licences/core-notices.txt`, are in step with `Cargo.lock`: a dependency
//! added, removed or moved to another version fails here until the file is
//! written again with `python3 tools/licences/notices.py`.

use std::path::PathBuf;
use std::process::Command;

#[test]
fn the_notices_of_the_core_are_in_step_with_the_lock_file() {
    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = Command::new("python3")
        .arg(repository.join("tools/licences/notices.py"))
        .arg("--check")
        .env("CARGO", env!("CARGO"))
        .current_dir(&repository)
        .output()
        .expect("python3 runs");
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
