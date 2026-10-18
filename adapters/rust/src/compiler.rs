use crate::profile::Profile;
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

pub const PIN: &str = "nightly-2026-10-15";
pub const HASH: &str = "7df81381d027710708209f36b98d11b1c88f8d2e";
pub const RELEASE: &str = "1.101.0-nightly";

#[derive(Debug)]
pub struct Error {
    pub code: &'static str,
    pub message: String,
}

pub struct Compiler {
    program: PathBuf,
    pub version: String,
}

impl Compiler {
    pub fn discover() -> Result<Self, Error> {
        // An explicit executable is useful in CI; it is never accepted without
        // checking the full compiler identity. No rustup operation installs anything.
        let candidates = if let Some(p) = std::env::var_os("SKE_RUSTC") {
            vec![PathBuf::from(p)]
        } else {
            let locate = |alias: &str| -> Option<PathBuf> {
                let o = Command::new("rustup")
                    .args(["which", "--toolchain", alias, "rustc"])
                    .output()
                    .ok()?;
                o.status
                    .success()
                    .then(|| PathBuf::from(String::from_utf8_lossy(&o.stdout).trim()))
            };
            [PIN, "nightly"]
                .iter()
                .filter_map(|alias| locate(alias))
                .collect()
        };
        let mut observed = vec![];
        for program in candidates {
            let out = match Command::new(&program).arg("-vV").output() {
                Ok(out) => out,
                Err(e) => {
                    observed.push(e.to_string());
                    continue;
                }
            };
            let version = String::from_utf8_lossy(&out.stdout).into_owned();
            if out.status.success()
                && version.lines().any(|l| l == format!("commit-hash: {HASH}"))
                && version.lines().any(|l| l == format!("release: {RELEASE}"))
            {
                return Ok(Self { program, version });
            }
            observed.push(version);
        }
        Err(Error {
            code: "TOOLCHAIN_MISMATCH",
            message: format!("required {PIN} release {RELEASE} hash {HASH}; observed: {}; no automatic installation; set SKE_RUSTC to a matching compiler", observed.join("; ")),
        })
    }

    pub fn mir(&self, root: &Path, p: &Profile) -> Result<String, Error> {
        let root = root.canonicalize().map_err(|e| Error {
            code: "SOURCE_INVALID",
            message: e.to_string(),
        })?;
        let entry = root.join(&p.entry).canonicalize().map_err(|e| Error {
            code: "SOURCE_INVALID",
            message: e.to_string(),
        })?;
        if !entry.starts_with(&root) {
            return Err(Error {
                code: "SOURCE_INVALID",
                message: "entry escapes project root".into(),
            });
        }
        let source = std::fs::read_to_string(entry).map_err(|e| Error {
            code: "SOURCE_INVALID",
            message: e.to_string(),
        })?;
        self.mir_source(&source, p, Duration::from_secs(600))
    }

    pub fn mir_source(
        &self,
        source: &str,
        p: &Profile,
        timeout: Duration,
    ) -> Result<String, Error> {
        // Recheck the source gate even for direct API callers. The compiler only
        // sees an immutable, single-file snapshot, never a mutable studied tree.
        crate::source::inventory(&p.entry, source).map_err(|message| Error {
            code: "RUST_SOURCE_UNSUPPORTED",
            message,
        })?;
        let scratch = tempfile::tempdir().map_err(|e| Error {
            code: "IO_ERROR",
            message: e.to_string(),
        })?;
        let entry = scratch.path().join("studied.rs");
        std::fs::write(&entry, source).map_err(|e| Error {
            code: "IO_ERROR",
            message: e.to_string(),
        })?;
        let stdout_path = scratch.path().join("mir.txt");
        let stderr_path = scratch.path().join("stderr.txt");
        let stdout = std::fs::File::create(&stdout_path).map_err(|e| Error {
            code: "IO_ERROR",
            message: e.to_string(),
        })?;
        let stderr = std::fs::File::create(&stderr_path).map_err(|e| Error {
            code: "IO_ERROR",
            message: e.to_string(),
        })?;
        // A private working directory prevents relative include! paths from
        // accidentally referring to adapter files. No user rustc flags/externs,
        // Cargo, build scripts, proc-macro dylibs, linker or emitted executable.
        let mut command = Command::new(&self.program);
        command
            .current_dir(scratch.path())
            .env_remove("RUSTC_BOOTSTRAP")
            .args([
                "--crate-name",
                "ske_studied",
                "--crate-type",
                "lib",
                "--edition=2021",
                "-Zunpretty=mir",
                "-Zmir-opt-level=0",
                "-Zmir-include-spans=yes",
                "-Copt-level=0",
                "-Cpanic=unwind",
                // Without this, the debug camera injects overflow-check assert
                // terminators into + - * << >> and negation, making loop counters
                // and simple arithmetic MIR_UNSUPPORTED (fail-closed). Release MIR
                // has no such checks (wrapping arithmetic), so this is the faithful
                // view; div/rem-by-zero asserts remain real panics and stay.
                "-Coverflow-checks=off",
            ])
            .arg(entry)
            .stdin(Stdio::null())
            .stdout(stdout)
            .stderr(stderr);
        if let Some(target) = &p.target {
            command.args(["--target", target]);
        }
        for cfg in &p.cfg {
            command.args(["--cfg", cfg]);
        }
        for feature in &p.features {
            command.args([
                "--cfg",
                &format!("feature={}", serde_json::to_string(feature).unwrap()),
            ]);
        }
        let mut child = command.spawn().map_err(|e| Error {
            code: "COMPILER_FAILED",
            message: e.to_string(),
        })?;
        let deadline = Instant::now() + timeout.min(Duration::from_secs(600));
        let status = loop {
            let exceeded = [&stdout_path, &stderr_path].iter().any(|p| {
                std::fs::metadata(p)
                    .map(|m| m.len() > 64 * 1024 * 1024)
                    .unwrap_or(true)
            });
            if Instant::now() >= deadline || exceeded {
                let _ = child.kill();
                let _ = child.wait();
                return Err(Error {
                    code: "LIMIT_EXCEEDED",
                    message: "compiler timeout or 64 MiB output limit exceeded".into(),
                });
            }
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) => std::thread::sleep(Duration::from_millis(10)),
                Err(e) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(Error {
                        code: "COMPILER_FAILED",
                        message: e.to_string(),
                    });
                }
            }
        };
        if !status.success() {
            return Err(Error {
                code: "COMPILER_FAILED",
                message: std::fs::read_to_string(stderr_path).unwrap_or_else(|e| e.to_string()),
            });
        }
        std::fs::read_to_string(stdout_path).map_err(|e| Error {
            code: "COMPILER_FAILED",
            message: e.to_string(),
        })
    }
}
