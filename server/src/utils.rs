use colored::Color;
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

#[macro_export]
macro_rules! pluralize {
    ($num: expr) => {
        if $num > 1 {
            "s"
        } else {
            ""
        }
    };
}

pub fn parse_bool(str: &str) -> Option<bool> {
    if str.eq_ignore_ascii_case("true") || str.eq_ignore_ascii_case("yes") {
        return Some(true);
    }
    if str.eq_ignore_ascii_case("false") || str.eq_ignore_ascii_case("no") {
        return Some(false);
    }
    None
}
