use std::collections::HashSet;
use std::sync::Arc;

use crate::convert::Expr;
use crate::core::{Poly, PolyCtx, Rat};
use crate::error::{Error, Result};
use crate::symbols::{Word, legacy};

#[derive(Clone, Debug)]
pub struct ParseResult {
    pub expr: Expr,
    pub ctx: Arc<PolyCtx>,
    pub augmented_vars: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TokenKind {
    End,
    Number,
    Ident,
    Plus,
    Minus,
    Star,
    Slash,
    Caret,
    LeftBracket,
    RightBracket,
    LeftBrace,
    RightBrace,
    LeftParen,
    RightParen,
    Comma,
}

#[derive(Clone, Debug)]
struct Token {
    kind: TokenKind,
    text: String,
    position: usize,
}

fn parse_error(message: impl Into<String>) -> Error {
    Error::InvalidInput(format!("parse: {}", message.into()))
}

fn is_function_name(identifier: &str) -> bool {
    matches!(identifier, "Hlog" | "Log" | "PolyLog")
}

fn tokenize(input: &str) -> Result<Vec<Token>> {
    let bytes = input.as_bytes();
    let mut position = 0;
    let mut tokens = Vec::new();

    while position < bytes.len() {
        let byte = bytes[position];
        if byte.is_ascii_whitespace() {
            position += 1;
            continue;
        }
        let start = position;
        if byte.is_ascii_digit() {
            position += 1;
            while position < bytes.len() && bytes[position].is_ascii_digit() {
                position += 1;
            }
            tokens.push(Token {
                kind: TokenKind::Number,
                text: input[start..position].into(),
                position: start,
            });
            continue;
        }
        if byte.is_ascii_alphabetic() || byte == b'_' {
            position += 1;
            while position < bytes.len()
                && (bytes[position].is_ascii_alphanumeric() || bytes[position] == b'_')
            {
                position += 1;
            }
            let mut end = position;
            let name = &input[start..end];

            // Preserve Mathematica integer subscripts (`m[1,2]`) as one
            // identifier. PolyCtx performs the final backend-name validation.
            if !is_function_name(name) && position < bytes.len() && bytes[position] == b'[' {
                let mut scan = position + 1;
                let mut saw_digit = false;
                let mut valid = true;
                while scan < bytes.len() && bytes[scan] != b']' {
                    match bytes[scan] {
                        digit if digit.is_ascii_digit() => saw_digit = true,
                        b',' | b' ' | b'\t' => {}
                        _ => {
                            valid = false;
                            break;
                        }
                    }
                    scan += 1;
                }
                if valid && saw_digit && scan < bytes.len() && bytes[scan] == b']' {
                    position = scan + 1;
                    end = position;
                }
            }
            tokens.push(Token {
                kind: TokenKind::Ident,
                text: input[start..end].into(),
                position: start,
            });
            continue;
        }

        let kind = match byte {
            b'+' => TokenKind::Plus,
            b'-' => TokenKind::Minus,
            b'*' => TokenKind::Star,
            b'/' => TokenKind::Slash,
            b'^' => TokenKind::Caret,
            b'[' => TokenKind::LeftBracket,
            b']' => TokenKind::RightBracket,
            b'{' => TokenKind::LeftBrace,
            b'}' => TokenKind::RightBrace,
            b'(' => TokenKind::LeftParen,
            b')' => TokenKind::RightParen,
            b',' => TokenKind::Comma,
            _ => {
                return Err(parse_error(format!(
                    "unexpected character `{}` at position {position}",
                    char::from(byte)
                )));
            }
        };
        position += 1;
        tokens.push(Token {
            kind,
            text: input[start..position].into(),
            position: start,
        });
    }
    tokens.push(Token {
        kind: TokenKind::End,
        text: String::new(),
        position,
    });
    Ok(tokens)
}

fn collect_variables(tokens: &[Token], user_variables: &[String]) -> Vec<String> {
    let mut variables = user_variables.to_vec();
    for token in tokens {
        if token.kind == TokenKind::Ident
            && !is_function_name(&token.text)
            && !variables.iter().any(|variable| variable == &token.text)
        {
            variables.push(token.text.clone());
        }
    }
    variables
}

struct Parser<'a> {
    tokens: &'a [Token],
    index: usize,
    ctx: Arc<PolyCtx>,
    lazy_top_sum: bool,
}

impl<'a> Parser<'a> {
    fn new(tokens: &'a [Token], ctx: Arc<PolyCtx>, lazy_top_sum: bool) -> Self {
        Self {
            tokens,
            index: 0,
            ctx,
            lazy_top_sum,
        }
    }

    fn peek(&self, offset: usize) -> &Token {
        self.tokens
            .get(self.index + offset)
            .unwrap_or_else(|| self.tokens.last().expect("tokenizer always emits End"))
    }

    fn consume(&mut self) -> Token {
        let token = self.peek(0).clone();
        if token.kind != TokenKind::End {
            self.index += 1;
        }
        token
    }

    fn expect(&mut self, kind: TokenKind, description: &str) -> Result<()> {
        if self.peek(0).kind != kind {
            return Err(parse_error(format!(
                "expected {description} at position {} but got `{}`",
                self.peek(0).position,
                self.peek(0).text
            )));
        }
        self.consume();
        Ok(())
    }

    fn zero(&self) -> Rat {
        Rat::zero(self.ctx.clone())
    }

    fn one(&self) -> Rat {
        Rat::one(self.ctx.clone())
    }

    fn parse(mut self) -> Result<Expr> {
        let expression = self.parse_sum(true)?;
        if self.peek(0).kind != TokenKind::End {
            return Err(parse_error(format!(
                "unexpected token `{}` at position {} (expected end of input)",
                self.peek(0).text,
                self.peek(0).position
            )));
        }
        Ok(expression)
    }

    fn parse_sum(&mut self, top_level: bool) -> Result<Expr> {
        let mut terms = vec![self.parse_product()?];
        while matches!(self.peek(0).kind, TokenKind::Plus | TokenKind::Minus) {
            let negate = self.consume().kind == TokenKind::Minus;
            let mut right = self.parse_product()?;
            if negate {
                right = match right {
                    Expr::Leaf(value) => Expr::Leaf(value.negated()),
                    value => {
                        Expr::Times(vec![Expr::Leaf(Rat::from_int(self.ctx.clone(), -1)), value])
                    }
                };
            }
            terms.push(right);
        }
        if terms.len() == 1 {
            return Ok(terms.pop().expect("one term"));
        }
        if !(top_level && self.lazy_top_sum)
            && terms.iter().all(|term| matches!(term, Expr::Leaf(_)))
        {
            let mut sum = self.zero();
            for term in terms {
                let Expr::Leaf(value) = term else {
                    unreachable!()
                };
                sum = sum.try_add(&value)?;
            }
            Ok(Expr::Leaf(sum))
        } else {
            Ok(Expr::Plus(terms))
        }
    }

    fn parse_product(&mut self) -> Result<Expr> {
        let mut factors = vec![self.parse_unary()?];
        while matches!(self.peek(0).kind, TokenKind::Star | TokenKind::Slash) {
            let invert = self.consume().kind == TokenKind::Slash;
            let right = self.parse_unary()?;
            let right = if invert {
                match right {
                    Expr::Leaf(value) => Expr::Leaf(self.one().try_div(&value)?),
                    _ => {
                        return Err(parse_error(
                            "division by a non-rational expression is unsupported",
                        ));
                    }
                }
            } else {
                right
            };
            factors.push(right);
        }
        if factors.len() == 1 {
            return Ok(factors.pop().expect("one factor"));
        }
        if factors.iter().all(|factor| matches!(factor, Expr::Leaf(_))) {
            let mut product = self.one();
            for factor in factors {
                let Expr::Leaf(value) = factor else {
                    unreachable!()
                };
                product = product.try_mul(&value)?;
            }
            Ok(Expr::Leaf(product))
        } else {
            Ok(Expr::Times(factors))
        }
    }

    fn parse_unary(&mut self) -> Result<Expr> {
        match self.peek(0).kind {
            TokenKind::Plus => {
                self.consume();
                self.parse_unary()
            }
            TokenKind::Minus => {
                self.consume();
                let inner = self.parse_unary()?;
                Ok(match inner {
                    Expr::Leaf(value) => Expr::Leaf(value.negated()),
                    value => {
                        Expr::Times(vec![Expr::Leaf(Rat::from_int(self.ctx.clone(), -1)), value])
                    }
                })
            }
            _ => self.parse_power(),
        }
    }

    fn parse_power(&mut self) -> Result<Expr> {
        let base = self.parse_call()?;
        if self.peek(0).kind != TokenKind::Caret {
            return Ok(base);
        }
        self.consume();
        // Parsing the exponent through unary makes powers right-associative
        // and gives `^` higher precedence than a leading minus on the base.
        let exponent = self.parse_unary()?;
        let Expr::Leaf(exponent) = exponent else {
            return Err(parse_error("exponent must be an integer literal"));
        };
        let exponent_string = exponent.to_string();
        let exponent = exponent_string.parse::<i64>().map_err(|_| {
            parse_error(format!(
                "non-integer exponent `{exponent_string}`; only integers are supported"
            ))
        })?;
        if exponent == 0 {
            return Ok(Expr::Leaf(self.one()));
        }
        match base {
            Expr::Leaf(value) => Ok(Expr::Leaf(value.pow(exponent)?)),
            _ if exponent < 0 => Err(parse_error(
                "negative exponent on a non-rational expression is unsupported",
            )),
            value => Expr::power(value, exponent),
        }
    }

    fn parse_call(&mut self) -> Result<Expr> {
        if self.peek(0).kind != TokenKind::Ident
            || !is_function_name(&self.peek(0).text)
            || self.peek(1).kind != TokenKind::LeftBracket
        {
            return self.parse_atom();
        }
        let name = self.consume().text;
        self.consume();
        match name.as_str() {
            "Log" => {
                let arg = self.parse_sum(false)?;
                self.expect(TokenKind::RightBracket, "`]` to close Log")?;
                let Expr::Leaf(arg) = arg else {
                    return Err(parse_error("Log argument must be rational"));
                };
                Ok(Expr::hlog(
                    arg,
                    Word::new(vec![Rat::zero(self.ctx.clone())]),
                ))
            }
            "Hlog" => {
                let arg = self.parse_sum(false)?;
                self.expect(TokenKind::Comma, "`,` between Hlog arguments")?;
                let Expr::Leaf(arg) = arg else {
                    return Err(parse_error("Hlog first argument must be rational"));
                };
                let (close, close_description) = match self.peek(0).kind {
                    TokenKind::LeftBracket => (TokenKind::RightBracket, "`]`"),
                    TokenKind::LeftBrace => (TokenKind::RightBrace, "`}`"),
                    _ => {
                        return Err(parse_error(
                            "expected `[` or `{` to open Hlog's letter list",
                        ));
                    }
                };
                self.consume();
                let mut letters = Vec::new();
                if self.peek(0).kind != close {
                    loop {
                        let letter = self.parse_sum(false)?;
                        let Expr::Leaf(letter) = letter else {
                            return Err(parse_error("Hlog letters must be rational"));
                        };
                        letters.push(letter);
                        if self.peek(0).kind != TokenKind::Comma {
                            break;
                        }
                        self.consume();
                    }
                }
                self.expect(
                    close,
                    &format!("{close_description} to close Hlog's letter list"),
                )?;
                self.expect(TokenKind::RightBracket, "`]` to close Hlog")?;
                Ok(Expr::hlog(arg, Word::new(letters)))
            }
            "PolyLog" => {
                let _index = self.parse_sum(false)?;
                self.expect(TokenKind::Comma, "`,` between PolyLog arguments")?;
                let _argument = self.parse_sum(false)?;
                self.expect(TokenKind::RightBracket, "`]` to close PolyLog")?;
                Err(parse_error(
                    "PolyLog conversion requires MplAsHlog and is not supported here",
                ))
            }
            _ => unreachable!(),
        }
    }

    fn parse_atom(&mut self) -> Result<Expr> {
        match self.peek(0).kind {
            TokenKind::LeftParen => {
                self.consume();
                let expression = self.parse_sum(false)?;
                self.expect(TokenKind::RightParen, "`)` to close grouping")?;
                Ok(expression)
            }
            TokenKind::Number => {
                let token = self.consume();
                Ok(Expr::Leaf(Rat::parse(self.ctx.clone(), &token.text)?))
            }
            TokenKind::Ident => {
                let token = self.consume();
                if is_function_name(&token.text) {
                    return Err(parse_error(format!(
                        "function name `{}` is not followed by `[`",
                        token.text
                    )));
                }
                let atom = legacy::atom_from_name(&token.text)?;
                let variable =
                    self.ctx
                        .index_of_indeterminate(atom.as_view())
                        .ok_or_else(|| {
                            parse_error(format!(
                                "identifier `{}` was not added to the polynomial context",
                                token.text
                            ))
                        })?;
                // Construct identifiers from their context generator rather
                // than reparsing their source spelling.  This is essential
                // for Mathematica integer-subscript names such as `m[1,2]`:
                // Symbolica's expression parser would otherwise interpret the
                // spelling as a function call, while PolyCtx intentionally
                // stores it as one inert polynomial variable.
                Ok(Expr::Leaf(Rat::from_poly(Poly::generator(
                    self.ctx.clone(),
                    variable,
                )?)))
            }
            _ => Err(parse_error(format!(
                "expected an atom at position {} but got `{}`",
                self.peek(0).position,
                self.peek(0).text
            ))),
        }
    }
}

pub fn parse_expression(
    input: &str,
    user_variables: &[String],
    lazy_top_sum: bool,
) -> Result<ParseResult> {
    let tokens = tokenize(input)?;
    let names = collect_variables(&tokens, user_variables);
    let mut augmented_vars = Vec::with_capacity(names.len());
    let mut indeterminates = Vec::with_capacity(names.len());
    let mut seen_names = HashSet::with_capacity(names.len());
    let mut seen_atoms = HashSet::with_capacity(names.len());
    for name in names {
        if name.is_empty() || !seen_names.insert(name.clone()) {
            return Err(Error::InvalidInput(format!(
                "variable names must be non-empty and unique: `{name}`"
            )));
        }
        let atom = legacy::atom_from_name(&name)?;
        // Reserved spellings such as mzv_3 and indexed input MZV[3]
        // denote one native indeterminate. Keep the first diagnostic name;
        // Parser::parse_atom resolves every spelling by that same identity.
        // Full Atom equality preserves distinct namespaces and function heads.
        if seen_atoms.insert(atom.clone()) {
            augmented_vars.push(name);
            indeterminates.push(atom);
        }
    }
    let ctx = PolyCtx::from_named_indeterminates(augmented_vars.clone(), indeterminates)?;
    let expr = Parser::new(&tokens, ctx.clone(), lazy_top_sum).parse()?;
    Ok(ParseResult {
        expr,
        ctx,
        augmented_vars,
    })
}

#[cfg(test)]
mod tests;
