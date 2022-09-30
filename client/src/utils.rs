use std::time::{Duration, SystemTime, UNIX_EPOCH};
use colored::Color;
use openssl::pkey::{Private, Public};
use openssl::rsa::Rsa;
use std::fmt::{Debug, Display};
use std::io::{Error, ErrorKind, Write};
use crate::ui;

pub const LIGHT_GRAY_TERM: Color = Color::TrueColor {
    r: 153,
    g: 153,
    b: 153,
};

pub const LIGHT_GRAY_GPU: wgpu::Color = wgpu::Color {
    r: 0.384,
    g: 0.396,
    b: 0.412,
    a: 1.0,
};

pub const DARK_GRAY_UI: ui::Color = ui::Color {
    r: 0.224,
    g: 0.239,
    b: 0.278,
    a: 1.0,
};

pub fn current_time_millis() -> Duration {
    let start = SystemTime::now();
    let since_the_epoch = start
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards");
    since_the_epoch
}

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
