//! Conformance helper: compare decoded CIR fields across language runtimes,
//! then re-encode the Go result with Rust for the return trip.
use prost::Message;
use ske_cir::proto::Record;
use std::{
    fs::File,
    io::{self, BufReader, BufWriter, Read, Write},
    path::Path,
};

fn read(reader: &mut impl Read) -> io::Result<Option<Record>> {
    let mut prefix = [0; 4];
    if reader.read(&mut prefix[..1])? == 0 {
        return Ok(None);
    }
    reader.read_exact(&mut prefix[1..])?;
    let length = u32::from_be_bytes(prefix) as usize;
    if length == 0 || length > 16 * 1024 * 1024 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid frame length",
        ));
    }
    let mut bytes = vec![0; length];
    reader.read_exact(&mut bytes)?;
    Record::decode(bytes.as_slice())
        .map(Some)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

fn compare(original: &Path, go: &Path, rust: &Path) -> io::Result<()> {
    let mut original = BufReader::new(File::open(original)?);
    let mut go = BufReader::new(File::open(go)?);
    let mut rust = BufWriter::new(File::create(rust)?);
    loop {
        match (read(&mut original)?, read(&mut go)?) {
            (None, None) => return rust.flush(),
            (Some(left), Some(right)) if left == right => {
                let bytes = right.encode_to_vec();
                rust.write_all(&(bytes.len() as u32).to_be_bytes())?;
                rust.write_all(&bytes)?;
            }
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "CIR semantic mismatch",
                ))
            }
        }
    }
}

fn main() -> io::Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 3 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "compare_records ORIGINAL GO_ROUNDTRIP RUST_ROUNDTRIP",
        ));
    }
    compare(
        Path::new(&args[0]),
        Path::new(&args[1]),
        Path::new(&args[2]),
    )
}
