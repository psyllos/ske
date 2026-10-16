//! Shared stable errors and canonical hashing. No dependency on the other crates.
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fmt;

pub mod diagnostics;
pub mod errors;
pub use errors::ErrorCode;

pub fn sha256_hex(bytes: impl AsRef<[u8]>) -> String {
    digest_hex(Sha256::digest(bytes.as_ref()))
}

/// Hash a source or executable with bounded memory, preserving I/O failures.
pub fn sha256_reader(mut reader: impl std::io::Read) -> std::io::Result<String> {
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => return Ok(digest_hex(digest.finalize())),
            Ok(n) => digest.update(&buffer[..n]),
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
}

fn digest_hex(bytes: impl IntoIterator<Item = u8>) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut result = String::with_capacity(64);
    for byte in bytes {
        result.push(HEX[(byte >> 4) as usize] as char);
        result.push(HEX[(byte & 15) as usize] as char);
    }
    result
}

#[cfg(test)]
mod reader_tests {
    use super::*;
    use std::io::{Error, ErrorKind, Read};

    struct InterruptedChunks {
        interrupted: bool,
        cursor: std::io::Cursor<&'static [u8]>,
    }
    impl Read for InterruptedChunks {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            if !std::mem::replace(&mut self.interrupted, true) {
                return Err(Error::from(ErrorKind::Interrupted));
            }
            self.cursor.read(&mut buffer[..1])
        }
    }

    #[test]
    fn sha256_reader_handles_short_and_interrupted_reads() {
        let input = InterruptedChunks {
            interrupted: false,
            cursor: std::io::Cursor::new(b"abc"),
        };
        assert_eq!(
            sha256_reader(input).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn sha256_reader_preserves_io_error() {
        struct Broken;
        impl Read for Broken {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                Err(Error::from(ErrorKind::PermissionDenied))
            }
        }
        assert_eq!(
            sha256_reader(Broken).unwrap_err().kind(),
            ErrorKind::PermissionDenied
        );
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Error {
    pub code: String,
    pub message: String,
    pub candidates: Vec<String>,
}

impl Error {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            candidates: Vec::new(),
        }
    }

    pub fn exit_code(&self) -> i32 {
        ErrorCode::from_code(&self.code).map_or(5, ErrorCode::exit_code)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for Error {}
pub type Result<T> = std::result::Result<T, Error>;

/// RFC 8785 serialization for SKE configuration documents (integer-only).
///
/// Object names are ordered by UTF-16 code units, not Rust's UTF-8 ordering.
/// Integers outside the interoperable IEEE-754 range are rejected instead of
/// silently rounding a fingerprint input.
pub fn canonical_json(value: &serde_json::Value) -> Result<String> {
    fn append(value: &serde_json::Value, out: &mut String) -> Result<()> {
        use serde_json::Value;
        match value {
            Value::Object(map) => {
                let mut entries: Vec<_> = map.iter().collect();
                entries
                    .sort_by(|(left, _), (right, _)| left.encode_utf16().cmp(right.encode_utf16()));
                out.push('{');
                for (index, (key, value)) in entries.into_iter().enumerate() {
                    if index != 0 {
                        out.push(',');
                    }
                    out.push_str(
                        &serde_json::to_string(key)
                            .map_err(|e| Error::new("E_INTERNAL", e.to_string()))?,
                    );
                    out.push(':');
                    append(value, out)?;
                }
                out.push('}');
            }
            Value::Array(values) => {
                out.push('[');
                for (index, value) in values.iter().enumerate() {
                    if index != 0 {
                        out.push(',');
                    }
                    append(value, out)?;
                }
                out.push(']');
            }
            Value::Number(number) => {
                const SAFE_INTEGER: u64 = (1_u64 << 53) - 1;
                let safe = number.as_u64().is_some_and(|n| n <= SAFE_INTEGER)
                    || number
                        .as_i64()
                        .is_some_and(|n| n.unsigned_abs() <= SAFE_INTEGER);
                if !safe {
                    return Err(Error::new(
                        "E_USAGE",
                        "Canonical configuration JSON requires safe integers",
                    ));
                }
                out.push_str(&number.to_string());
            }
            _ => out.push_str(
                &serde_json::to_string(value)
                    .map_err(|e| Error::new("E_INTERNAL", e.to_string()))?,
            ),
        }
        Ok(())
    }
    let mut out = String::new();
    append(value, &mut out)?;
    Ok(out)
}

pub fn local_key(unit_key: &str, stream_id: &str) -> String {
    format!("{unit_key}|{stream_id}")
}

pub fn format_span(
    path: &str,
    start_line: u32,
    start_column: u32,
    end_line: u32,
    end_column: u32,
) -> String {
    format!("{path}:{start_line}:{start_column}-{end_line}:{end_column}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_registry_roundtrip() {
        for &code in ErrorCode::ALL {
            assert_eq!(ErrorCode::from_code(code.as_str()), Some(code));
            assert_ne!(Error::new(code.as_str(), "").exit_code(), 1);
        }
        assert_eq!(Error::new("unregistered", "").exit_code(), 5);
        assert_eq!(ErrorCode::ALL.len(), 47);
    }

    #[test]
    fn diagnostic_inventory_impact() {
        assert!(diagnostics::degrades_inventory("WARNING", "INVENTORY"));
        assert!(!diagnostics::degrades_inventory("WARNING", "SYMBOLIC"));
        assert!(!diagnostics::degrades_inventory("INFO", "INVENTORY"));
        assert_eq!(diagnostics::DIAGNOSTICS.len(), 31);
    }

    #[test]
    fn canonical_config_order_and_integer_contract() {
        let value = serde_json::json!({"z": [{"b": 2, "a": 1}], "\u{e000}": 0, "\u{1f600}": 0});
        assert_eq!(
            canonical_json(&value).unwrap(),
            "{\"z\":[{\"a\":1,\"b\":2}],\"😀\":0,\"\u{e000}\":0}"
        );
        assert!(canonical_json(&serde_json::json!({"n": 1.5})).is_err());
        assert!(canonical_json(&serde_json::json!({"n": 9007199254740993_u64})).is_err());
    }

    #[test]
    fn hashing_known_answer() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
