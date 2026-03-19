use serde::Serialize;
use std::io::Write;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Human,
    Json,
}

impl OutputFormat {
    pub fn from_json_flag(json: bool) -> Self {
        if json { Self::Json } else { Self::Human }
    }
}

#[derive(Serialize)]
pub struct CliOutput<T: Serialize> {
    pub status: &'static str,
    #[serde(flatten)]
    pub data: T,
}

#[derive(Serialize)]
pub struct CliError {
    pub status: &'static str,
    pub message: String,
}

pub fn print_json_value<T: Serialize>(
    format: OutputFormat,
    data: &T,
    human_display: impl FnOnce(),
) {
    match format {
        OutputFormat::Human => human_display(),
        OutputFormat::Json => {
            let output = CliOutput {
                status: "success",
                data,
            };
            println!(
                "{}",
                serde_json::to_string_pretty(&output).unwrap_or_default()
            );
        }
    }
}

pub fn print_error(format: OutputFormat, message: &str) {
    match format {
        OutputFormat::Human => {
            eprintln!("Error: {message}");
        }
        OutputFormat::Json => {
            let output = CliError {
                status: "error",
                message: message.to_string(),
            };
            println!(
                "{}",
                serde_json::to_string_pretty(&output).unwrap_or_default()
            );
        }
    }
}

pub fn print_list<T: Serialize>(format: OutputFormat, items: &[T], human_display: impl FnOnce()) {
    match format {
        OutputFormat::Human => human_display(),
        OutputFormat::Json => {
            let output = CliOutput {
                status: "success",
                data: items,
            };
            println!(
                "{}",
                serde_json::to_string_pretty(&output).unwrap_or_default()
            );
        }
    }
}

pub fn status_message(msg: &str) {
    let mut stderr = std::io::stderr();
    let _ = writeln!(stderr, "{msg}");
}
