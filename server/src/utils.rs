use colored::Color;
use std::fmt::{Debug, Display};
use std::io::{Error, ErrorKind, Write};
use openssl::pkey::{Private, Public};
use openssl::rsa::Rsa;

pub const LIGHT_GRAY: Color = Color::TrueColor {
    r: 153,
    g: 153,
    b: 153,
};

/// prompts for input in the console with a specific message
pub fn input(text: &Option<impl Display>) -> std::io::Result<String> {
    if let Some(text) = text {
        print!("{}", text);
        print!(": ");
    }
    std::io::stdout().flush()?; // because print! doesn't flush
    let mut input = String::new();
    if std::io::stdin().read_line(&mut input)? == 0 {
        return Err(Error::new(
            ErrorKind::UnexpectedEof,
            "EOF while reading a line",
        ));
    }
    if input.ends_with('\n') {
        input.pop();
        if input.ends_with('\r') {
            input.pop();
        }
    }
    Ok(input)
}
