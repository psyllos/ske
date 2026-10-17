pub fn decide(x: i32) -> i32 {
    if x < 0 {
        -1
    } else if x < -10 {
        -2
    } else {
        0
    }
}

pub fn kind(n: u8) -> &'static str {
    match n {
        0 => "zero",
        1..=9 => "small",
        _ => "big",
    }
}

pub fn parse_len(s: &str) -> Result<usize, std::num::ParseIntError> {
    let n: usize = s.parse()?;
    Ok(n)
}
