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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn accepts_input_at_the_bound() {
        let input = read_bounded(Cursor::new("1234"), 4).expect("bounded input");
        assert_eq!(input, "1234");
    }

    #[test]
    fn rejects_input_over_the_bound() {
        let error = read_bounded(Cursor::new("12345"), 4).expect_err("oversized input");
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }
}
