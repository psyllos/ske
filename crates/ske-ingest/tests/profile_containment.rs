//! Profile path containment must be enforced before any adapter subprocess.
use ske_ingest::{profile, Language};

fn rust(entry: &str) -> String {
    format!(
        r#"{{"profile_version":1,"language":"rust","target":null,"features":[],"cfg":[],"entry":"{entry}"}}"#
    )
}

fn java(class_path: &str, source_roots: &str) -> String {
    format!(
        r#"{{"profile_version":1,"language":"java","class_path":["{class_path}"],"source_roots":["{source_roots}"],"target_release":17}}"#
    )
}

#[test]
fn profile_containment_rejects_escapes() {
    let root = std::path::PathBuf::from("/repo/root");
    for (language, profile, field) in [
        (Language::Rust, rust(".."), "entry"),
        (Language::Rust, rust("../outside.rs"), "entry"),
        (Language::Rust, rust("/etc/passwd"), "entry"),
        (
            Language::Java,
            java("/tmp/outside.jar", "src/main/java"),
            "class_path",
        ),
        (
            Language::Java,
            java("target/classes", "../../outside"),
            "source_roots",
        ),
        (
            Language::Java,
            java("libs/*.jar", "src/main/java"),
            "class_path",
        ),
    ] {
        let error =
            profile::validate_contained(language, &profile, &root).expect_err("must reject");
        assert_eq!(error.code, "E_USAGE", "{}", error.message);
        assert!(
            error.message.contains(field),
            "expected {field:?} in {}",
            error.message
        );
    }
    // A properly JSON-escaped backslash reaches reject_escaping's backslash check.
    let escaped = r#"{"profile_version":1,"language":"rust","target":null,"features":[],"cfg":[],"entry":"src\\evil.rs"}"#;
    let error =
        profile::validate_contained(Language::Rust, escaped, &root).expect_err("must reject");
    assert_eq!(error.code, "E_USAGE", "{}", error.message);
    assert!(error.message.contains("backslash"), "{}", error.message);
}

#[test]
fn profile_containment_accepts_internal_relative_paths() {
    let root = std::path::PathBuf::from("/repo/root");
    profile::validate_contained(Language::Rust, &rust("src/lib.rs"), &root)
        .expect("src/lib.rs is valid");
    profile::validate_contained(
        Language::Java,
        &java("target/classes", "src/main/java"),
        &root,
    )
    .expect("internal Java classpath is valid");
    profile::validate_contained(Language::Go, "{}", &root).expect("Go has no path fields");
}
