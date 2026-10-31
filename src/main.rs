use e1647::ast::Member;
use std::env;
use std::fs;
use std::process::ExitCode;

const USAGE: &str = "Usage: e1647 <command> <source.e> [--seed N]\nCommands:\n  tokens  tokenize the source (does not parse)\n  parse   parse the source and print the AST or diagnostics\n  elab    parse and fold like/extend into elaborated types\n  run     parse, elaborate and execute sys.run() (--seed N for reproducible gen)\n\n`import x.y;` members resolve to `x/y.e` (or `x.y.e`) next to the source\nfile; imported declarations are spliced before the importing file's own.";

/// Parse `source` and splice in the files it imports (`import a.b;` →
/// `a/b.e` or `a.b.e` beside the source), depth-first, one level wide.
/// Imported declarations precede the importing file's own, mirroring
/// Specman's load order. Unresolvable imports stay in the AST (the
/// existing E0101/E0203 diagnostics name them) — no silent drops.
fn parse_with_imports(path: &str, source: &str) -> e1647::parser::ParseResult {
    let result = e1647::parser::parse_unchecked(source);
    let mut declarations = Vec::new();
    // Imported files' diagnostics come first (their declarations precede
    // ours); spans carry no file name yet, so they render under the
    // importing file's path — a known limitation of the flat ParseResult.
    let mut diagnostics = Vec::new();
    let dir = std::path::Path::new(path)
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_default();
    for declaration in &result.declarations {
        for member in &declaration.members {
            let Member::Import { names, .. } = member else {
                continue;
            };
            for name in names {
                let candidates = [
                    dir.join(format!("{name}.e")),
                    dir.join(name.replace('.', "/")).with_extension("e"),
                    dir.join(name.replace('.', "-")).with_extension("e"),
                ];
                let mut loaded = false;
                for candidate in &candidates {
                    let Ok(imported) = fs::read_to_string(candidate) else {
                        continue;
                    };
                    let imported =
                        parse_with_imports(candidate.to_string_lossy().as_ref(), &imported);
                    diagnostics.extend(imported.diagnostics);
                    declarations.extend(imported.declarations);
                    loaded = true;
                    break;
                }
                if !loaded {
                    let (start, end) = declaration.span;
                    diagnostics.push(e1647::diagnostic::Diagnostic::new(
                        "E0205",
                        format!("no file found for imported module '{name}'"),
                        start,
                        end,
                    ));
                }
            }
        }
    }
    declarations.extend(result.declarations);
    diagnostics.extend(result.diagnostics);
    // The well-formedness pass runs once over the MERGED declaration set
    // so cross-file references (imports) resolve.
    diagnostics.extend(e1647::check::check_declarations(&declarations));
    e1647::parser::ParseResult {
        declarations,
        diagnostics,
    }
}

fn run(command: &str, path: &str, seed: u64) -> ExitCode {
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
            let result = parse_with_imports(path, &source);
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
            let result = parse_with_imports(path, &source);
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
        "run" => {
            let result = parse_with_imports(path, &source);
            for diagnostic in &result.diagnostics {
                eprintln!("{path}:{}", diagnostic.render());
            }
            if !result.diagnostics.is_empty() {
                return ExitCode::FAILURE;
            }
            let elaborated = e1647::elab::elaborate(&result.declarations);
            for diagnostic in &elaborated.diagnostics {
                eprintln!("{path}:{}", diagnostic.render());
            }
            if !elaborated.diagnostics.is_empty() {
                return ExitCode::FAILURE;
            }
            let outcome = e1647::interp::run(&elaborated, seed);
            for diagnostic in &outcome.diagnostics {
                eprintln!("{path}:{}", diagnostic.render());
            }
            print!("{}", outcome.output);
            if outcome.diagnostics.is_empty() {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        _ => {
            eprintln!("Unknown command: {command}. Supported: tokens, parse, elab, run");
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
    let mut seed: u64 = 1;
    let mut path: Option<String> = None;
    while let Some(arg) = args.next() {
        if arg == "--seed" {
            let Some(value) = args.next() else {
                eprintln!("--seed requires a value");
                return ExitCode::FAILURE;
            };
            match value.parse() {
                Ok(parsed) => seed = parsed,
                Err(_) => {
                    eprintln!("invalid seed: {value}");
                    return ExitCode::FAILURE;
                }
            }
        } else if let Some(value) = arg.strip_prefix("--seed=") {
            match value.parse() {
                Ok(parsed) => seed = parsed,
                Err(_) => {
                    eprintln!("invalid seed: {value}");
                    return ExitCode::FAILURE;
                }
            }
        } else if path.is_none() {
            path = Some(arg);
        } else {
            eprintln!("Too many arguments");
            return ExitCode::FAILURE;
        }
    }
    let Some(path) = path else {
        eprintln!("{USAGE}");
        return ExitCode::FAILURE;
    };
    run(&command, &path, seed)
}

fn render_type(ty: &e1647::ast::TypeExpr) -> String {
    use e1647::ast::{Expr, TypeExpr};
    match ty {
        TypeExpr::Scalar { base, width, .. } => match width.as_deref() {
            Some(Expr::IntLiteral { text, .. }) => format!("{}(bits: {text})", base.name()),
            Some(_) => format!("{}(bits: <expr>)", base.name()),
            None => base.name().to_owned(),
        },
        TypeExpr::List { element, key, .. } => match key {
            Some(_) => format!("list (key: <expr>) of {}", render_type(element)),
            None => format!("list of {}", render_type(element)),
        },
        TypeExpr::Named { name, .. } => name.clone(),
        TypeExpr::InlineEnum { names, .. } => format!("[{}]", names.join(", ")),
        TypeExpr::Formal { name, .. } => format!("<{name}>"),
        TypeExpr::TemplateRef { name, args, .. } => {
            let args = args.iter().map(render_type).collect::<Vec<_>>().join(", ");
            format!("{name} of {args}")
        }
    }
}

fn print_method(method: &e1647::ast::MethodDecl, indent: &str) {
    let params = method
        .params
        .iter()
        .map(|param| match &param.ty {
            Some(ty) => format!("{}: {}", param.name, render_type(ty)),
            None => param.name.clone(),
        })
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
    use e1647::ast::{Expr, Member, PortDirection};
    for member in members {
        match member {
            Member::Field(field) => {
                println!("{indent}field {}: {}", field.name, render_type(&field.ty));
            }
            Member::Cover(cover) => {
                println!(
                    "{indent}cover {} ({} items, line {})",
                    if cover.name.is_empty() {
                        "<expr>"
                    } else {
                        &cover.name
                    },
                    cover.items.len(),
                    cover.span.0.line
                );
            }
            Member::Port(port) => {
                let direction = match port.direction {
                    PortDirection::In => "in",
                    PortDirection::Out => "out",
                    PortDirection::Inout => "inout",
                };
                println!(
                    "{indent}port {}: {} simple_port of {}",
                    port.name,
                    direction,
                    render_type(&port.element)
                );
            }
            Member::Event(event) => {
                println!("{indent}event {} (line {})", event.name, event.span.0.line)
            }
            Member::On(on) => {
                println!(
                    "{indent}on {} ({} statements, line {})",
                    on.path.join("."),
                    on.body.len(),
                    on.span.0.line
                );
            }
            Member::Expect(expect) => println!(
                "{indent}{} ({} else statements, line {})",
                match expect.kind {
                    e1647::ast::ExpectKind::Expect => "expect",
                    e1647::ast::ExpectKind::Assume => "assume",
                },
                expect.else_body.len(),
                expect.span.0.line
            ),
            Member::Import { names, .. } => {
                println!("{indent}import {}", names.join(", "));
            }
            Member::Define { name, .. } => println!("{indent}define {name}"),
            Member::Macro(macro_decl) => println!(
                "{indent}macro <{}{}> ({} body tokens, line {})",
                macro_decl.name,
                macro_decl
                    .category
                    .as_ref()
                    .map(|category| format!("'{category}"))
                    .unwrap_or_default(),
                macro_decl.body.len(),
                macro_decl.span.0.line
            ),
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
