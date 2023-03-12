use colored::Color;
use openssl::pkey::{Private, Public};
use openssl::rsa::Rsa;
use std::fmt::{Debug, Display};
use std::io::{Error, ErrorKind, Write};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const LIGHT_GRAY: Color = Color::TrueColor {
    r: 153,
    g: 153,
    b: 153,
};

pub fn current_time_millis() -> Duration {
    let start = SystemTime::now();
    let since_the_epoch = start
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards");
    since_the_epoch
}

/// prompts for input in the console with a specific message
pub fn input<T: Display>(text: &Option<T>) -> std::io::Result<String> {
    if let Some(text) = text {
        print!("{}: ", text);
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
