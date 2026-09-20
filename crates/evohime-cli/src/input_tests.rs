use crate::input::read_bounded;
use std::io::{self, Cursor};

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
