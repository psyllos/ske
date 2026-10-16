use ske_ingest::adapter::{list_units, RunLimits};
use std::{path::Path, process::Command};

#[test]
fn adapter_list_units_orders_packages_before_build_suffixes() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let temp = tempfile::tempdir().unwrap();
    let executable = temp.path().join("ske-adapter-go");
    // Build only our trusted adapter, never the project being studied.
    let build = Command::new("go")
        .current_dir(root.join("adapters/go"))
        .args(["build", "-o"])
        .arg(&executable)
        .arg("./cmd/ske-adapter-go")
        .output()
        .expect("Go is a v0.2 prerequisite");
    assert!(
        build.status.success(),
        "adapter build failed: {}",
        String::from_utf8_lossy(&build.stderr)
    );
    let profile =
        std::fs::read_to_string(root.join("examples/go-branch-lab/build-profile.json")).unwrap();
    let units = list_units(
        &executable,
        &root.join("examples/go-branch-lab"),
        &profile,
        &RunLimits::default(),
    )
    .expect("discovery must accept the specification's package ordering");
    assert_eq!(
        units
            .iter()
            .map(|u| u.unit_key.as_str())
            .collect::<Vec<_>>(),
        [
            "go:example@linux/amd64",
            "go:example/partial@linux/amd64",
            "go:example/tags@linux/amd64",
        ]
    );
}
