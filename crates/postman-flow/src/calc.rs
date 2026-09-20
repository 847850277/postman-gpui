//! Bounded arithmetic expressions shared by static validation and runtime evaluation.

const MAX_DEPTH: usize = 64;
const MAX_TOKENS: usize = 4096;
const MAX_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub enum CalcError {
    ParseError(String),
    EvalError(String),
}

impl std::fmt::Display for CalcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ParseError(msg) => write!(f, "calc parse error: {msg}"),
            Self::EvalError(msg) => write!(f, "calc eval error: {msg}"),
        }
    }
}

impl std::error::Error for CalcError {}

pub fn evaluate_calc(
    expression: &str,
    lookup: impl Fn(&str) -> Result<f64, String>,
) -> Result<f64, CalcError> {
    Expression::parse(expression)?.evaluate(lookup)
}

// A flat postfix program avoids recursive evaluation/drop of long arithmetic chains.
pub(crate) struct Expression {
    instructions: Vec<Instruction>,
}

enum Instruction {
    Number(f64),
    Variable(String),
    Unary(Unary),
    Binary(Binary),
}

enum Unary {
    Negate,
    Floor,
    Ceil,
    Round,
    Abs,
}
enum Binary {
    Add,
    Subtract,
    Multiply,
    Divide,
}

impl Expression {
    pub(crate) fn parse(source: &str) -> Result<Self, CalcError> {
        let mut parser = Parser {
            tokens: tokenize(source)?,
            pos: 0,
            instructions: Vec::new(),
        };
        parser.expr(0)?;
        if parser.peek().is_some() {
            return Err(CalcError::ParseError("unexpected trailing token".into()));
        }
        Ok(Self {
            instructions: parser.instructions,
        })
    }

    pub(crate) fn variables(&self) -> impl Iterator<Item = &str> {
        self.instructions
            .iter()
            .filter_map(|instruction| match instruction {
                Instruction::Variable(name) => Some(name.as_str()),
                _ => None,
            })
    }

    fn evaluate(&self, lookup: impl Fn(&str) -> Result<f64, String>) -> Result<f64, CalcError> {
        let mut stack = Vec::new();
        for instruction in &self.instructions {
            let value = match instruction {
                Instruction::Number(value) => *value,
                Instruction::Variable(name) => lookup(name).map_err(CalcError::EvalError)?,
                Instruction::Unary(op) => {
                    let value: f64 = stack.pop().expect("validated unary operand");
                    match op {
                        Unary::Negate => -value,
                        Unary::Floor => value.floor(),
                        Unary::Ceil => value.ceil(),
                        Unary::Round => value.round(),
                        Unary::Abs => value.abs(),
                    }
                }
                Instruction::Binary(op) => {
                    let right = stack.pop().expect("validated right operand");
                    let left = stack.pop().expect("validated left operand");
                    match op {
                        Binary::Add => left + right,
                        Binary::Subtract => left - right,
                        Binary::Multiply => left * right,
                        Binary::Divide => {
                            if right == 0.0 {
                                return Err(CalcError::EvalError("division by zero".into()));
                            }
                            left / right
                        }
                    }
                }
            };
            if !value.is_finite() {
                return Err(CalcError::EvalError(
                    "calculation requires finite numbers and results".into(),
                ));
            }
            stack.push(value);
        }
        Ok(stack.pop().expect("validated expression has a result"))
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Number(f64),
    Ident(String),
    Plus,
    Minus,
    Star,
    Slash,
    LParen,
    RParen,
}

fn ident_start(ch: char) -> bool {
    ch.is_alphabetic() || ch == '_' || ch == '$'
}
fn ident_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_' || ch == '$'
}

fn tokenize(source: &str) -> Result<Vec<Token>, CalcError> {
    if source.len() > MAX_BYTES {
        return Err(CalcError::ParseError("expression exceeds 64 KiB".into()));
    }
    let chars: Vec<char> = source.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        if ch.is_whitespace() {
            i += 1;
            continue;
        }
        if tokens.len() == MAX_TOKENS {
            return Err(CalcError::ParseError(
                "expression exceeds 4096 tokens".into(),
            ));
        }
        let token = match ch {
            '+' => Token::Plus,
            '-' => Token::Minus,
            '*' => Token::Star,
            '/' => Token::Slash,
            '(' => Token::LParen,
            ')' => Token::RParen,
            '`' => {
                i += 1;
                let mut name = String::new();
                while i < chars.len() && chars[i] != '`' {
                    if chars[i] == '\\' {
                        i += 1;
                    }
                    if i < chars.len() {
                        name.push(chars[i]);
                        i += 1;
                    }
                }
                if i == chars.len() || name.is_empty() {
                    return Err(CalcError::ParseError(
                        "expected a nonempty, closed backtick reference".into(),
                    ));
                }
                Token::Ident(name)
            }
            '0'..='9' | '.' => {
                let start = i;
                while i < chars.len() && chars[i].is_ascii_digit() {
                    i += 1;
                }
                if chars.get(i) == Some(&'.') {
                    i += 1;
                    while i < chars.len() && chars[i].is_ascii_digit() {
                        i += 1;
                    }
                }
                if matches!(chars.get(i), Some('e' | 'E')) {
                    i += 1;
                    if matches!(chars.get(i), Some('+' | '-')) {
                        i += 1;
                    }
                    while i < chars.len() && chars[i].is_ascii_digit() {
                        i += 1;
                    }
                }
                let value: String = chars[start..i].iter().collect();
                let number = value
                    .parse::<f64>()
                    .map_err(|_| CalcError::ParseError("invalid number".into()))?;
                if !number.is_finite() {
                    return Err(CalcError::ParseError("number must be finite".into()));
                }
                tokens.push(Token::Number(number));
                continue;
            }
            _ if ident_start(ch) => {
                let start = i;
                while i < chars.len() && ident_char(chars[i]) {
                    i += 1;
                }
                // Preserve existing step-1.output syntax. Elsewhere '-' is subtraction;
                // input/output names containing '-' can be quoted with backticks.
                let mut step_end = i;
                while step_end < chars.len()
                    && (ident_char(chars[step_end]) || chars[step_end] == '-')
                {
                    step_end += 1;
                }
                if chars.get(step_end) == Some(&'.')
                    && chars.get(step_end + 1).copied().is_some_and(ident_start)
                {
                    i = step_end + 1;
                    while i < chars.len() && (ident_char(chars[i]) || chars[i] == '.') {
                        i += 1;
                    }
                }
                tokens.push(Token::Ident(chars[start..i].iter().collect()));
                continue;
            }
            _ => {
                return Err(CalcError::ParseError(format!(
                    "unexpected character '{ch}'"
                )))
            }
        };
        tokens.push(token);
        i += 1;
    }
    Ok(tokens)
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    instructions: Vec<Instruction>,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn advance(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.pos)?.clone();
        self.pos += 1;
        Some(token)
    }

    fn expr(&mut self, depth: usize) -> Result<(), CalcError> {
        self.term(depth)?;
        while let Some(Token::Plus | Token::Minus) = self.peek() {
            let op = if self.advance() == Some(Token::Plus) {
                Binary::Add
            } else {
                Binary::Subtract
            };
            self.term(depth)?;
            self.instructions.push(Instruction::Binary(op));
        }
        Ok(())
    }

    fn term(&mut self, depth: usize) -> Result<(), CalcError> {
        self.factor(depth)?;
        while let Some(Token::Star | Token::Slash) = self.peek() {
            let op = if self.advance() == Some(Token::Star) {
                Binary::Multiply
            } else {
                Binary::Divide
            };
            self.factor(depth)?;
            self.instructions.push(Instruction::Binary(op));
        }
        Ok(())
    }

    fn factor(&mut self, depth: usize) -> Result<(), CalcError> {
        if depth > MAX_DEPTH {
            return Err(CalcError::ParseError(
                "expression may nest at most 64 levels".into(),
            ));
        }
        match self.advance() {
            Some(Token::Plus) => self.factor(depth + 1)?,
            Some(Token::Minus) => {
                self.factor(depth + 1)?;
                self.instructions.push(Instruction::Unary(Unary::Negate));
            }
            Some(Token::Number(value)) => self.instructions.push(Instruction::Number(value)),
            Some(Token::LParen) => {
                self.expr(depth + 1)?;
                self.close_paren()?;
            }
            Some(Token::Ident(name)) => {
                if self.peek() == Some(&Token::LParen) {
                    self.advance();
                    let op = match name.to_ascii_lowercase().as_str() {
                        "floor" => Unary::Floor,
                        "ceil" => Unary::Ceil,
                        "round" => Unary::Round,
                        "abs" => Unary::Abs,
                        _ => {
                            return Err(CalcError::ParseError(format!("unknown function '{name}'")))
                        }
                    };
                    self.expr(depth + 1)?;
                    self.close_paren()?;
                    self.instructions.push(Instruction::Unary(op));
                } else {
                    self.instructions.push(Instruction::Variable(name));
                }
            }
            _ => {
                return Err(CalcError::ParseError(
                    "expected a number, reference or '('".into(),
                ))
            }
        }
        Ok(())
    }

    fn close_paren(&mut self) -> Result<(), CalcError> {
        if self.advance() == Some(Token::RParen) {
            Ok(())
        } else {
            Err(CalcError::ParseError("expected ')'".into()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arithmetic_references_and_subtraction() {
        let lookup = |name: &str| match name {
            "balance" => Ok(100.0),
            "price" => Ok(8.5),
            "step-1.usdt" => Ok(50.0),
            "risk-factor" => Ok(2.0),
            _ => Err(format!("unknown variable {name}")),
        };
        for (expression, expected) in [
            ("1 + 2 * 3", 7.0),
            ("(1 + 2) * 3", 9.0),
            ("floor((step-1.usdt * 2) / price)", 11.0),
            ("balance-1", 99.0),
            ("step-1.usdt-1", 49.0),
            ("balance - 1.5", 98.5),
            ("balance-1.5", 98.5),
            ("`risk-factor`-1", 1.0),
            ("abs(-2) + ceil(1.1) + round(2.6)", 7.0),
            ("1e2 + .5 - 2.5e-1", 100.25),
        ] {
            assert_eq!(
                evaluate_calc(expression, lookup).unwrap(),
                expected,
                "{expression}"
            );
        }
    }

    #[test]
    fn malformed_and_deep_expressions_return_errors() {
        for expression in [
            "",
            " ",
            "(",
            "floor(",
            "1+",
            "floor()",
            "(1",
            "1 2",
            "unknown(1)",
            "1,2",
            "`unclosed",
            "``",
            ".",
            "1e+",
        ] {
            assert!(Expression::parse(expression).is_err(), "{expression:?}");
        }
        for expression in [
            format!("{}1{}", "(".repeat(1000), ")".repeat(1000)),
            format!("{}1", "-".repeat(1000)),
            "1+".repeat(5000),
        ] {
            assert!(Expression::parse(&expression).is_err());
        }
        let flat = vec!["1"; 1000].join("+");
        assert_eq!(evaluate_calc(&flat, |_| unreachable!()).unwrap(), 1000.0);
    }

    #[test]
    fn nonfinite_values_overflow_and_division_by_zero_fail() {
        for expression in ["1/0", "1e308*2", "1e309", "value", "value*0"] {
            assert!(
                evaluate_calc(expression, |_| Ok(f64::INFINITY)).is_err(),
                "{expression}"
            );
        }
        assert!(evaluate_calc("value", |_| Ok(f64::NAN)).is_err());
    }
}
