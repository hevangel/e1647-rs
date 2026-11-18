//! Resolve supported initialized const fields and schema-time references.
//! Runtime filters retain their original expressions and source scope.

use super::*;
use crate::elab::ElaboratedType;

pub(super) const MAX_STRING_BYTES: usize = 256 * 1024;

#[derive(Clone)]
enum Value {
    Int(i128),
    Str(String),
}

impl Value {
    fn token_kind(self) -> TokenKind {
        match self {
            Self::Int(value) => TokenKind::Number(value.to_string()),
            Self::Str(value) => TokenKind::StringLiteral(
                value
                    .replace('\\', "\\\\")
                    .replace('"', "\\\"")
                    .replace('\n', "\\n")
                    .replace('\r', "\\r")
                    .replace('\t', "\\t")
                    .replace('\0', "\\0"),
            ),
        }
    }
    fn expression(self, span: crate::ast::Span) -> Expr {
        match self.token_kind() {
            TokenKind::Number(text) => Expr::IntLiteral { text, span },
            TokenKind::StringLiteral(value) => Expr::StringLiteral { value, span },
            _ => unreachable!("constant literal"),
        }
    }
}

struct Resolver<'a> {
    types: &'a [ElaboratedType],
    memo: BTreeMap<(String, String), Option<Value>>,
    active: BTreeSet<(String, String)>,
    diagnostics: Vec<crate::diagnostic::Diagnostic>,
}

impl Resolver<'_> {
    fn scalar_type(&mut self, owner: &str, ty: &TypeExpr, depth: usize) -> Option<TypeExpr> {
        if depth + self.active.len() > 64 {
            return None;
        }
        match ty {
            TypeExpr::Named { name, .. } => {
                let underlying = self
                    .types
                    .iter()
                    .find(|ty| ty.name == *name && ty.kind == crate::ast::DeclarationKind::Type)?
                    .underlying
                    .as_ref()?
                    .clone();
                self.scalar_type(owner, &underlying, depth + 1)
            }
            TypeExpr::Scalar { base, width, span }
                if matches!(
                    base,
                    ScalarBase::Int
                        | ScalarBase::Uint
                        | ScalarBase::Longint
                        | ScalarBase::Longuint
                        | ScalarBase::String
                ) =>
            {
                let width = match width.as_deref() {
                    None => None,
                    Some(Expr::Ident { name, .. })
                        if name == "*" && matches!(base, ScalarBase::Int | ScalarBase::Uint) =>
                    {
                        width.clone()
                    }
                    Some(expr) if matches!(base, ScalarBase::Int | ScalarBase::Uint) => {
                        let Value::Int(bits) = self.evaluate(owner, expr, 0)? else {
                            return None;
                        };
                        if !(1..=127).contains(&bits) {
                            return None;
                        }
                        Some(Box::new(Expr::IntLiteral {
                            text: bits.to_string(),
                            span: expr.span(),
                        }))
                    }
                    _ => return None,
                };
                Some(TypeExpr::Scalar {
                    base: *base,
                    width,
                    span: *span,
                })
            }
            _ => None,
        }
    }

    fn operand_type(&mut self, owner: &str, expr: &Expr) -> Option<TypeExpr> {
        let (owner, name) = match expr {
            Expr::Ident { name, .. } => (owner, name.as_str()),
            Expr::StaticAccess {
                type_name, member, ..
            } => (type_name.as_str(), member.as_str()),
            _ => return None,
        };
        let ty = self
            .types
            .iter()
            .find(|ty| ty.name == owner)?
            .fields
            .iter()
            .find(|field| field.name == name)?
            .ty
            .clone();
        self.scalar_type(owner, &ty, 0)
    }

    fn field(&mut self, owner: &str, name: &str) -> Option<Value> {
        let key = (owner.to_owned(), name.to_owned());
        if let Some(value) = self.memo.get(&key) {
            return value.clone();
        }
        if self.active.len() >= 64 || !self.active.insert(key.clone()) {
            return None;
        }
        let value = self
            .types
            .iter()
            .find(|ty| ty.name == owner)
            .and_then(|ty| ty.fields.iter().find(|field| field.name == name))
            .filter(|field| field.is_const)
            .cloned()
            .and_then(|field| {
                let ty = self.scalar_type(owner, &field.ty, 0)?;
                let value = self.evaluate(owner, field.init.as_ref()?, 0)?;
                // Storage narrowing is not inferred. Only resolve explicitly
                // sized initializers when they already fit that representation.
                if let (
                    TypeExpr::Scalar {
                        base,
                        width: Some(width),
                        ..
                    },
                    Value::Int(value),
                ) = (&ty, &value)
                    && let Expr::IntLiteral { text, .. } = width.as_ref()
                {
                    let bits: u32 = text.parse().ok()?;
                    let fits = if *base == ScalarBase::Uint {
                        *value >= 0 && *value <= (i128::MAX >> (127 - bits))
                    } else {
                        let half = 1i128.checked_shl(bits - 1)?;
                        *value >= -half && *value < half
                    };
                    if !fits {
                        return None;
                    }
                }
                Some(value)
            });
        self.active.remove(&key);
        if value.is_some() {
            self.memo.insert(key, value.clone());
        }
        value
    }

    fn evaluate(&mut self, owner: &str, expr: &Expr, depth: usize) -> Option<Value> {
        if depth + self.active.len() > 64 {
            return None;
        }
        match expr {
            Expr::IntLiteral { text, .. } | Expr::BitLiteral { text, .. } => {
                crate::interp::parse_int_literal(text).map(Value::Int)
            }
            Expr::StringLiteral { value, .. } => Some(Value::Str(crate::interp::unescape(value))),
            Expr::Ident { name, .. } => self.field(owner, name).or_else(|| {
                // An owner field shadows a predefined constant even when its
                // initializer is not supported or it is mutable.
                if self
                    .types
                    .iter()
                    .find(|ty| ty.name == owner)
                    .is_some_and(|ty| ty.fields.iter().any(|field| field.name == *name))
                {
                    return None;
                }
                let value = match name.as_str() {
                    "MAX_INT" => i32::MAX as i128,
                    "MIN_INT" => i32::MIN as i128,
                    "MAX_UINT" => u32::MAX as i128,
                    "MAX_LONGINT" => i64::MAX as i128,
                    "MIN_LONGINT" => i64::MIN as i128,
                    "MAX_LONGUINT" => u64::MAX as i128,
                    _ => return None,
                };
                Some(Value::Int(value))
            }),
            Expr::StaticAccess {
                type_name, member, ..
            } => {
                let field = self
                    .types
                    .iter()
                    .find(|ty| ty.name == *type_name)?
                    .fields
                    .iter()
                    .find(|field| field.name == *member)?;
                if !field.is_static {
                    return None;
                }
                self.field(type_name, member)
            }
            Expr::Paren { expr, .. } => self.evaluate(owner, expr, depth + 1),
            Expr::Unary { op, expr, .. } => {
                let Value::Int(value) = self.evaluate(owner, expr, depth + 1)? else {
                    return None;
                };
                match op {
                    crate::ast::UnaryOp::Plus => Some(Value::Int(value)),
                    crate::ast::UnaryOp::Negate => value.checked_neg().map(Value::Int),
                    _ => None,
                }
            }
            Expr::Binary { op, lhs, rhs, .. } => {
                let lhs_type = self.operand_type(owner, lhs);
                let rhs_type = self.operand_type(owner, rhs);
                let precision =
                    crate::interp::precision_context(lhs_type.as_ref(), rhs_type.as_ref());
                let lhs = self.evaluate(owner, lhs, depth + 1)?;
                let rhs = self.evaluate(owner, rhs, depth + 1)?;
                use crate::ast::BinaryOp;
                match (lhs, rhs) {
                    (Value::Str(mut lhs), Value::Str(rhs)) if *op == BinaryOp::Add => {
                        if lhs.len().checked_add(rhs.len())? > MAX_STRING_BYTES {
                            return None;
                        }
                        lhs.push_str(&rhs);
                        Some(Value::Str(lhs))
                    }
                    (Value::Int(lhs), Value::Int(rhs)) => {
                        let lhs = precision.cast_operand(lhs);
                        let rhs = precision.cast_operand(rhs);
                        let result = match op {
                            BinaryOp::Add if precision.bits().is_some() => {
                                Some(lhs.wrapping_add(rhs))
                            }
                            BinaryOp::Sub if precision.bits().is_some() => {
                                Some(lhs.wrapping_sub(rhs))
                            }
                            BinaryOp::Mul if precision.bits().is_some() => {
                                Some(lhs.wrapping_mul(rhs))
                            }
                            BinaryOp::Add => lhs.checked_add(rhs),
                            BinaryOp::Sub => lhs.checked_sub(rhs),
                            BinaryOp::Mul => lhs.checked_mul(rhs),
                            BinaryOp::Div => lhs.checked_div(rhs),
                            BinaryOp::Mod => lhs.checked_rem(rhs),
                            _ => None,
                        }?;
                        Some(Value::Int(precision.cast_operand(result)))
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn tokens(&mut self, owner: &str, tokens: &mut Vec<Token>) {
        // Fold a whole expression before replacing references: otherwise a
        // typed operand becomes an untyped literal and loses its precision.
        if let Ok(expr) = crate::parser::parse_expression_tokens(tokens)
            && let Some(value) = self.evaluate(owner, &expr, 0)
        {
            *tokens = vec![Token {
                kind: value.token_kind(),
                start: tokens[0].start,
                end: tokens.last().unwrap().end,
            }];
            return;
        }
        // Range clauses contain expressions inside delimiters and separated
        // by commas, semicolons or '..'. Keep those exact boundary tokens.
        let mut depth = 0usize;
        let mut start = 0;
        let mut pieces = Vec::new();
        for (index, token) in tokens.iter().enumerate() {
            if depth == 0
                && [",", ";", ".."]
                    .iter()
                    .any(|symbol| token_kind_is(token, symbol))
            {
                pieces.push((start, index));
                start = index + 1;
            } else if let TokenKind::Punctuation(symbol) = &token.kind {
                match symbol.as_str() {
                    "(" | "[" | "{" => depth += 1,
                    ")" | "]" | "}" => depth = depth.saturating_sub(1),
                    _ => {}
                }
            }
        }
        if !pieces.is_empty() {
            pieces.push((start, tokens.len()));
            let mut result = Vec::new();
            for (start, end) in pieces {
                let mut part = tokens[start..end].to_vec();
                self.tokens(owner, &mut part);
                result.extend(part);
                if end < tokens.len() {
                    result.push(tokens[end].clone());
                }
            }
            *tokens = result;
            return;
        }
        let mut result = Vec::with_capacity(tokens.len());
        let mut index = 0;
        while index < tokens.len() {
            let token = &tokens[index];
            if ["(", "[", "{"]
                .iter()
                .any(|symbol| token_kind_is(token, symbol))
            {
                let mut depth = 1usize;
                let mut end = index + 1;
                while end < tokens.len() && depth > 0 {
                    if let TokenKind::Punctuation(symbol) = &tokens[end].kind {
                        match symbol.as_str() {
                            "(" | "[" | "{" => depth += 1,
                            ")" | "]" | "}" => depth -= 1,
                            _ => {}
                        }
                    }
                    end += 1;
                }
                if depth == 0 {
                    let mut inner = tokens[index + 1..end - 1].to_vec();
                    self.tokens(owner, &mut inner);
                    result.push(token.clone());
                    result.extend(inner);
                    result.push(tokens[end - 1].clone());
                    index = end;
                    continue;
                }
            }
            let mut end = index + 1;
            let value = if let TokenKind::Identifier(name) = &token.kind {
                if tokens
                    .get(index + 1)
                    .is_some_and(|token| token_kind_is(token, "::"))
                {
                    end = (index + 3).min(tokens.len());
                    tokens
                        .get(index + 2)
                        .and_then(token_word)
                        .and_then(|member| {
                            let expr = Expr::StaticAccess {
                                type_name: name.clone(),
                                member: member.to_owned(),
                                member_span: (tokens[index + 2].start, tokens[index + 2].end),
                                span: (token.start, tokens[end - 1].end),
                            };
                            self.evaluate(owner, &expr, 0)
                        })
                } else {
                    self.field(owner, name)
                }
            } else {
                None
            };
            if value.is_none()
                && end == index + 1
                && let Some(name) = token_word(token)
                && matches!(
                    name,
                    "MAX_INT"
                        | "MIN_INT"
                        | "MAX_UINT"
                        | "MAX_LONGINT"
                        | "MIN_LONGINT"
                        | "MAX_LONGUINT"
                )
                && self
                    .types
                    .iter()
                    .find(|ty| ty.name == owner)
                    .is_some_and(|ty| ty.fields.iter().any(|field| field.name == name))
            {
                self.diagnostics.push(crate::diagnostic::Diagnostic::new(
                    "E0224",
                    format!(
                        "coverage constant '{name}' is shadowed by an unsupported field (14.3-14.5)"
                    ),
                    token.start,
                    token.end,
                ));
            }
            if let Some(value) = value {
                result.push(Token {
                    kind: value.token_kind(),
                    start: token.start,
                    end: tokens[end - 1].end,
                });
            } else {
                result.extend_from_slice(&tokens[index..end]);
            }
            index = end;
        }
        *tokens = result;
    }

    fn options(&mut self, owner: &str, tokens: &[Token]) -> Vec<Token> {
        let mut result = Vec::new();
        let mut start = 0;
        let mut depth = 0usize;
        for end in 0..=tokens.len() {
            let separator = end == tokens.len()
                || (depth == 0
                    && (token_kind_is(&tokens[end], ",") || token_kind_is(&tokens[end], ";")));
            if separator {
                let mut entry = tokens[start..end].to_vec();
                if entry
                    .first()
                    .and_then(token_word)
                    .is_some_and(|name| matches!(name, "name" | "ranges" | "at_least" | "weight"))
                {
                    let offset = if entry.get(1).is_some_and(|token| token_kind_is(token, "=")) {
                        2
                    } else {
                        1
                    };
                    let mut value = entry.split_off(offset);
                    self.tokens(owner, &mut value);
                    entry.extend(value);
                }
                result.extend(entry);
                if end < tokens.len() {
                    result.push(tokens[end].clone());
                }
                start = end + 1;
            } else if let TokenKind::Punctuation(symbol) = &tokens[end].kind {
                match symbol.as_str() {
                    "(" | "[" | "{" => depth += 1,
                    ")" | "]" | "}" => depth = depth.saturating_sub(1),
                    _ => {}
                }
            }
        }
        result
    }
}

pub(crate) fn scalar_type(
    types: &[ElaboratedType],
    owner: &str,
    ty: &TypeExpr,
) -> Option<TypeExpr> {
    Resolver {
        types,
        memo: BTreeMap::new(),
        active: BTreeSet::new(),
        diagnostics: Vec::new(),
    }
    .scalar_type(owner, ty, 0)
}

pub(crate) fn resolve(types: &mut [ElaboratedType]) -> Vec<crate::diagnostic::Diagnostic> {
    let mut resolver = Resolver {
        types,
        memo: BTreeMap::new(),
        active: BTreeSet::new(),
        diagnostics: Vec::new(),
    };
    let covers: Vec<_> = types
        .iter()
        .map(|ty| {
            let mut covers = ty.covers.clone();
            for group in &mut covers {
                for option in &mut group.options {
                    *option = resolver.options(&ty.name, option);
                }
                for item in &mut group.items {
                    item.using = resolver.options(&ty.name, &item.using);
                }
            }
            let inits: Vec<_> = ty
                .fields
                .iter()
                .map(|field| {
                    let span = field.init.as_ref()?.span();
                    resolver
                        .field(&ty.name, &field.name)
                        .map(|value| value.expression(span))
                })
                .collect();
            (covers, inits)
        })
        .collect();
    let diagnostics = resolver.diagnostics;
    for (ty, (covers, inits)) in types.iter_mut().zip(covers) {
        ty.covers = covers;
        for (field, init) in ty.fields.iter_mut().zip(inits) {
            if let Some(init) = init {
                field.init = Some(init);
            }
        }
    }
    diagnostics
}
