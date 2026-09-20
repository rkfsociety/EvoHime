use std::io::{self, Read};

pub(crate) fn read_bounded<R: Read>(reader: R, max_bytes: usize) -> io::Result<String> {
    let mut input = String::new();
    let mut reader = reader.take((max_bytes as u64).saturating_add(1));
    reader.read_to_string(&mut input)?;
    if input.len() > max_bytes {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "stdin exceeds the CLI input bound",
        ));
    }
    Ok(input)
}
