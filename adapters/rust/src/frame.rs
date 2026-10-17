use prost::Message;
use std::io::{self, Read, Write};

pub const LIMIT: usize = 16 * 1024 * 1024;

pub fn read<M: Message + Default>(r: &mut impl Read) -> Result<M, String> {
    let mut prefix = [0; 4];
    r.read_exact(&mut prefix).map_err(|e| e.to_string())?;
    let len = u32::from_be_bytes(prefix) as usize;
    if len == 0 || len > LIMIT {
        return Err(format!("frame length {len} exceeds limit {LIMIT}"));
    }
    let mut bytes = vec![0; len];
    r.read_exact(&mut bytes).map_err(|e| e.to_string())?;
    M::decode(bytes.as_slice()).map_err(|e| e.to_string())
}

pub fn write<M: Message>(w: &mut impl Write, value: &M, limit: usize) -> io::Result<()> {
    let bytes = value.encode_to_vec();
    if bytes.len() > limit.min(LIMIT) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "frame exceeds limit",
        ));
    }
    w.write_all(&(bytes.len() as u32).to_be_bytes())?;
    w.write_all(&bytes)?;
    w.flush()
}
