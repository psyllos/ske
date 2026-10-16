mod go_support;
use go_support::*;

#[test]
fn repeated_extractions_have_identical_framed_protobuf_bytes() {
    for unit in [
        "go:example@linux/amd64",
        "go:example/partial@linux/amd64",
        "go:example/tags@linux/amd64",
    ] {
        let first = extract(unit, "linux", "amd64");
        let second = extract(unit, "linux", "amd64");
        assert_eq!(first, second, "P-DET {unit}");
    }
}
