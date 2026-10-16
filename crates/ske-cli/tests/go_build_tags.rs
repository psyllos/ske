mod go_support;
use go_support::*;

#[test]
fn platform_profiles_isolate_sources_and_snapshot_identity() {
    let lab = Lab::empty();
    let linux = lab.ingest("linux", "amd64")["snapshot_id"]
        .as_i64()
        .unwrap();
    let darwin = lab.ingest("darwin", "amd64")["snapshot_id"]
        .as_i64()
        .unwrap();
    assert_ne!(linux, darwin);
    let c = lab.conn();
    let fingerprints = rows(&c, "SELECT config_fingerprint FROM snapshots ORDER BY id");
    assert_eq!(fingerprints.len(), 2);
    assert_ne!(fingerprints[0], fingerprints[1]);
    for (snapshot, included, excluded) in [
        (linux, "tags/tag_linux.go", "tags/tag_other.go"),
        (darwin, "tags/tag_other.go", "tags/tag_linux.go"),
    ] {
        // source_files is snapshot-scoped, not a membership table. Test all
        // tag-package paths, including files that have no decision spans.
        assert_eq!(rows(&c, &format!("SELECT path FROM source_files WHERE snapshot_id={snapshot} AND path LIKE 'tags/%' ORDER BY path")), vec![vec![included.to_owned()]]);
        let diagnostics = rows(&c, &format!("SELECT d.severity,d.message FROM diagnostics d JOIN compilation_units u ON u.id=d.unit_id WHERE u.snapshot_id={snapshot} AND u.unit_key LIKE 'go:example/tags@%' AND d.code='BUILD_TAG_EXCLUDED_FILE'"));
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0][0], "INFO");
        assert!(
            diagnostics[0][1].contains(excluded),
            "excluded path missing: {diagnostics:?}"
        );
    }
    lab.ok(&["db", "check"]);
}
