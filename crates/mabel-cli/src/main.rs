//! The `mabel` command line (proposal 001 section 9, ticket 008).
//!
//! Every command answers twice: text for a person, and with `--json` the
//! document `contracts/cli/` freezes. A failure answers with the error
//! envelope and exits with the code of the table in `contracts/README.md`: 2
//! usage, 10 schema, 20 ledger or policy, 30 network, 50 state or replay, 60
//! insecure key permissions, 70 unsupported.
//!
//! `--json` output goes to stdout, whether it succeeded or not, so a caller can
//! pipe one document either way. Text errors go to stderr.
//!
//! What this build does not carry yet: `sync push`, `sync fetch` and `wallet
//! serve` (tickets 011 and 012), and `identity rotate`, which exits 70 because
//! key rotation is out of scope (decision 008).

mod append;
mod artifacts;
mod cli;
mod commands;
mod context;
mod documents;
mod error;
mod ids;
mod ledger;
mod render;

use std::ffi::OsString;
use std::process::ExitCode;

use clap::error::{ContextKind, ContextValue, ErrorKind};
use clap::{CommandFactory, Parser};

use cli::Cli;
use error::CliError;

fn main() -> ExitCode {
    let arguments: Vec<OsString> = std::env::args_os().collect();
    // The flag has to be read before the parse that would reject it, so a
    // usage error is rendered in the form the caller asked for.
    let json = arguments.iter().any(|argument| argument == "--json");

    let cli = match Cli::try_parse_from(&arguments) {
        Ok(cli) => cli,
        Err(error) => return report_usage(&error, json),
    };
    if cli.verbose {
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .with_writer(std::io::stderr)
            .init();
    }

    match commands::run(&cli) {
        Ok(outcome) => {
            if cli.json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&outcome.document).unwrap_or_default()
                );
            } else if !outcome.text.is_empty() {
                println!("{}", outcome.text);
            }
            ExitCode::SUCCESS
        }
        Err(error) => report(&error, cli.json),
    }
}

/// Prints one failure and turns its exit code into an [`ExitCode`].
fn report(error: &CliError, json: bool) -> ExitCode {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&error.to_document()).unwrap_or_default()
        );
    } else {
        eprintln!("{}", error.message());
    }
    exit_code(error.exit_code())
}

/// A clap failure: help and version succeed, everything else is code 2.
///
/// Text mode prints clap's own rendering, which carries the usage line; JSON
/// mode prints the envelope, with `details.reason` naming the class of usage
/// error and `details.argument` the flag at fault.
fn report_usage(error: &clap::Error, json: bool) -> ExitCode {
    if matches!(
        error.kind(),
        ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
    ) {
        print!("{error}");
        return ExitCode::SUCCESS;
    }
    if !json {
        let _ = error.print();
        return exit_code(2);
    }
    report(&usage_error(error), true)
}

fn usage_error(error: &clap::Error) -> CliError {
    let argument = argument_at_fault(error);
    let (reason, message) = match error.kind() {
        ErrorKind::MissingRequiredArgument => (
            "missing_argument",
            format!(
                "missing required argument {}",
                argument.clone().unwrap_or_else(|| "unknown".to_owned())
            ),
        ),
        ErrorKind::UnknownArgument => (
            "unknown_argument",
            format!(
                "unknown argument {}",
                argument.clone().unwrap_or_else(|| "unknown".to_owned())
            ),
        ),
        ErrorKind::InvalidValue | ErrorKind::ValueValidation => (
            "invalid_value",
            format!(
                "invalid value for {}",
                argument.clone().unwrap_or_else(|| "an argument".to_owned())
            ),
        ),
        ErrorKind::InvalidSubcommand => (
            "unknown_command",
            "no such command; run mabel --help".to_owned(),
        ),
        ErrorKind::MissingSubcommand | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand => (
            "missing_command",
            "no command given; run mabel --help".to_owned(),
        ),
        _ => (
            "usage",
            first_line(&Cli::command().render_usage().to_string()),
        ),
    };
    let mut usage = CliError::usage(reason, message);
    if let Some(argument) = argument {
        usage = usage.with_detail("argument", argument);
    }
    usage
}

/// The flag clap named, trimmed of its value placeholder: `--subject`, not
/// `--subject <SUBJECT>`.
fn argument_at_fault(error: &clap::Error) -> Option<String> {
    let value = error.get(ContextKind::InvalidArg)?;
    let raw = match value {
        ContextValue::String(one) => one.clone(),
        ContextValue::Strings(many) => many.first()?.clone(),
        _ => return None,
    };
    Some(
        raw.split_whitespace()
            .next()
            .unwrap_or(raw.as_str())
            .to_owned(),
    )
}

fn first_line(text: &str) -> String {
    text.lines().next().unwrap_or_default().to_owned()
}

fn exit_code(code: i32) -> ExitCode {
    ExitCode::from(u8::try_from(code).unwrap_or(1))
}
