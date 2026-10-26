use std::env;
use std::fs;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let Some(command) = args.next() else {
        eprintln!("Usage: e1647 tokens <source.e>");
        return ExitCode::FAILURE;
    };
    if command != "tokens" {
        eprintln!("Unknown command: {command}. Supported: tokens");
        return ExitCode::FAILURE;
    }
    let Some(path) = args.next() else {
        eprintln!("Usage: e1647 tokens <source.e>");
        return ExitCode::FAILURE;
    };
    if args.next().is_some() {
        eprintln!("Too many arguments");
        return ExitCode::FAILURE;
    }
    let source = match fs::read_to_string(&path) {
        Ok(source) => source,
        Err(err) => {
            eprintln!("Cannot read {path}: {err}");
            return ExitCode::FAILURE;
        }
    };
    match e1647::lexer::tokenize(&source) {
        Ok(tokens) => {
            for token in tokens {
                println!(
                    "{}:{}  {:?}",
                    token.start.line, token.start.column, token.kind
                );
            }
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("Lexical error: {err}");
            ExitCode::FAILURE
        }
    }
}
