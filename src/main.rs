use std::env;
use std::fs;
use std::process::ExitCode;

const USAGE: &str = "Usage: e1647 <command> <source.e>\nCommands:\n  tokens  tokenize the source (does not parse)\n  parse   parse the source and print the AST or diagnostics";

fn run(command: &str, path: &str) -> ExitCode {
    let source = match fs::read_to_string(path) {
        Ok(source) => source,
        Err(err) => {
            eprintln!("Cannot read {path}: {err}");
            return ExitCode::FAILURE;
        }
    };
    match command {
        "tokens" => match e1647::lexer::tokenize(&source) {
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
        },
        "parse" => {
            let result = e1647::parser::parse(&source);
            for diagnostic in &result.diagnostics {
                eprintln!("{path}:{}", diagnostic.render());
            }
            if result.diagnostics.is_empty() {
                for declaration in &result.declarations {
                    println!("{declaration:#?}");
                }
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        _ => {
            eprintln!("Unknown command: {command}. Supported: tokens, parse");
            ExitCode::FAILURE
        }
    }
}

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let Some(command) = args.next() else {
        eprintln!("{USAGE}");
        return ExitCode::FAILURE;
    };
    if matches!(command.as_str(), "--help" | "-h") {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    let Some(path) = args.next() else {
        eprintln!("{USAGE}");
        return ExitCode::FAILURE;
    };
    if args.next().is_some() {
        eprintln!("Too many arguments");
        return ExitCode::FAILURE;
    }
    run(&command, &path)
}
