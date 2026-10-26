use std::env;
use std::fs;
use std::process::ExitCode;

const USAGE: &str = "Usage: e1647 <command> <source.e>\nCommands:\n  tokens  tokenize the source (does not parse)\n  parse   parse the source and print the AST or diagnostics\n  elab    parse and fold like/extend into elaborated types";

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
        "elab" => {
            let result = e1647::parser::parse(&source);
            for diagnostic in &result.diagnostics {
                eprintln!("{path}:{}", diagnostic.render());
            }
            let elaborated = e1647::elab::elaborate(&result.declarations);
            for diagnostic in &elaborated.diagnostics {
                eprintln!("{path}:{}", diagnostic.render());
            }
            if result.diagnostics.is_empty() && elaborated.diagnostics.is_empty() {
                for elaborated_type in &elaborated.types {
                    print_elaborated_type(elaborated_type);
                }
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        _ => {
            eprintln!("Unknown command: {command}. Supported: tokens, parse, elab");
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

fn render_type(ty: &e1647::ast::TypeExpr) -> String {
    use e1647::ast::{Expr, TypeExpr};
    match ty {
        TypeExpr::Scalar { base, width, .. } => match width.as_deref() {
            Some(Expr::IntLiteral { text, .. }) => format!("{}(bits: {text})", base.name()),
            Some(_) => format!("{}(bits: <expr>)", base.name()),
            None => base.name().to_owned(),
        },
        TypeExpr::List { element, .. } => format!("list of {}", render_type(element)),
        TypeExpr::Named { name, .. } => name.clone(),
    }
}

fn print_method(method: &e1647::ast::MethodDecl, indent: &str) {
    let params = method
        .params
        .iter()
        .map(|param| format!("{}: {}", param.name, render_type(&param.ty)))
        .collect::<Vec<_>>()
        .join(", ");
    let modifier = match method.modifier {
        Some(e1647::ast::MethodModifier::Also) => " is also",
        Some(e1647::ast::MethodModifier::First) => " is first",
        Some(e1647::ast::MethodModifier::Only) => " is only",
        None => "",
    };
    match &method.return_type {
        Some(return_type) => println!(
            "{indent}method {name}({params}): {ret}{modifier} ({} statements)",
            method.body.len(),
            name = method.name,
            ret = render_type(return_type),
        ),
        None => println!(
            "{indent}method {name}({params}){modifier} ({} statements)",
            method.body.len(),
            name = method.name,
        ),
    }
}

fn print_members(members: &[e1647::ast::Member], indent: &str) {
    use e1647::ast::{Expr, Member};
    for member in members {
        match member {
            Member::Field(field) => {
                println!("{indent}field {}: {}", field.name, render_type(&field.ty));
            }
            Member::Constraint(constraint) => println!(
                "{indent}keep{} (line {})",
                if constraint.is_soft { " soft" } else { "" },
                constraint.span.0.line
            ),
            Member::Method(method) => print_method(method, indent),
            Member::When(when) => {
                let modifiers = when
                    .modifiers
                    .iter()
                    .map(|modifier| match &modifier.id {
                        Some((id, _)) => format!("{}'{id}", modifier.value),
                        None => modifier.value.clone(),
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                println!("{indent}when {modifiers}");
                print_members(&when.members, &format!("{indent}  "));
            }
            Member::Enumeral(enumeral) => match &enumeral.value {
                Some(Expr::IntLiteral { text, .. }) => {
                    println!("{indent}enumeral {} = {text}", enumeral.name)
                }
                Some(_) => println!("{indent}enumeral {} = <expr>", enumeral.name),
                None => println!("{indent}enumeral {}", enumeral.name),
            },
        }
    }
}

fn print_elaborated_type(elaborated_type: &e1647::elab::ElaboratedType) {
    use e1647::ast::{DeclarationKind, Expr};
    let kind = match elaborated_type.kind {
        DeclarationKind::Struct => "struct",
        DeclarationKind::Unit => "unit",
        DeclarationKind::Type => "type",
        DeclarationKind::Extend => "extend",
    };
    match &elaborated_type.base {
        Some(base) => println!("{kind} {} like {base}", elaborated_type.name),
        None => println!("{kind} {}", elaborated_type.name),
    }
    for enumeral in &elaborated_type.enumerals {
        match &enumeral.value {
            Some(Expr::IntLiteral { text, .. }) => {
                println!("  enumeral {} = {text}", enumeral.name)
            }
            Some(_) => println!("  enumeral {} = <expr>", enumeral.name),
            None => println!("  enumeral {}", enumeral.name),
        }
    }
    for field in &elaborated_type.fields {
        println!("  field {}: {}", field.name, render_type(&field.ty));
    }
    for constraint in &elaborated_type.constraints {
        println!(
            "  keep{} (line {})",
            if constraint.is_soft { " soft" } else { "" },
            constraint.span.0.line
        );
    }
    for method in &elaborated_type.methods {
        print_method(method, "  ");
    }
    for when in &elaborated_type.whens {
        let modifiers = when
            .modifiers
            .iter()
            .map(|modifier| match &modifier.id {
                Some((id, _)) => format!("{}'{id}", modifier.value),
                None => modifier.value.clone(),
            })
            .collect::<Vec<_>>()
            .join(" ");
        println!("  when {modifiers}");
        print_members(&when.members, "    ");
    }
}
