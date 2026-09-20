//! Lightweight arithmetic expression parser and evaluator for flow steps.


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
    let tokens = tokenize(expression)?;
    let mut parser = Parser::new(tokens, lookup);
    let result = parser.parse_expr()?;
    if !parser.is_at_end() {
        return Err(CalcError::ParseError(format!(
            "unexpected token at end of expression: {:?}",
            parser.peek()
        )));
    }
    Ok(result)
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
    Comma,
}

fn tokenize(expr: &str) -> Result<Vec<Token>, CalcError> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = expr.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let ch = chars[i];
        if ch.is_whitespace() {
            i += 1;
            continue;
        }

        match ch {
            '+' => {
                tokens.push(Token::Plus);
                i += 1;
            }
            '-' => {
                tokens.push(Token::Minus);
                i += 1;
            }
            '*' => {
                tokens.push(Token::Star);
                i += 1;
            }
            '/' => {
                tokens.push(Token::Slash);
                i += 1;
            }
            '(' => {
                tokens.push(Token::LParen);
                i += 1;
            }
            ')' => {
                tokens.push(Token::RParen);
                i += 1;
            }
            ',' => {
                tokens.push(Token::Comma);
                i += 1;
            }
            '0'..='9' | '.' => {
                let start = i;
                let mut has_dot = ch == '.';
                i += 1;
                while i < chars.len() {
                    let c = chars[i];
                    if c == '.' {
                        if has_dot {
                            break;
                        }
                        has_dot = true;
                        i += 1;
                    } else if c.is_ascii_digit() {
                        i += 1;
                    } else {
                        break;
                    }
                }
                let s: String = chars[start..i].iter().collect();
                let num = s
                    .parse::<f64>()
                    .map_err(|e| CalcError::ParseError(format!("invalid number '{s}': {e}")))?;
                tokens.push(Token::Number(num));
            }
            _ if ch.is_alphabetic() || ch == '_' || ch == '$' => {
                let start = i;
                i += 1;
                while i < chars.len() {
                    let c = chars[i];
                    if c.is_alphanumeric() || c == '_' || c == '-' || c == '.' || c == '$' {
                        i += 1;
                    } else {
                        break;
                    }
                }
                let s: String = chars[start..i].iter().collect();
                tokens.push(Token::Ident(s));
            }
            _ => {
                return Err(CalcError::ParseError(format!("unexpected character '{ch}'")));
            }
        }
    }

    Ok(tokens)
}

struct Parser<F> {
    tokens: Vec<Token>,
    pos: usize,
    lookup: F,
}

impl<F: Fn(&str) -> Result<f64, String>> Parser<F> {
    fn new(tokens: Vec<Token>, lookup: F) -> Self {
        Self {
            tokens,
            pos: 0,
            lookup,
        }
    }

    fn is_at_end(&self) -> bool {
        self.pos >= self.tokens.len()
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn advance(&mut self) -> Option<&Token> {
        if !self.is_at_end() {
            self.pos += 1;
        }
        self.tokens.get(self.pos - 1)
    }

    fn parse_expr(&mut self) -> Result<f64, CalcError> {
        let mut left = self.parse_term()?;

        while let Some(op) = self.peek() {
            match op {
                Token::Plus => {
                    self.advance();
                    let right = self.parse_term()?;
                    left += right;
                }
                Token::Minus => {
                    self.advance();
                    let right = self.parse_term()?;
                    left -= right;
                }
                _ => break,
            }
        }

        Ok(left)
    }

    fn parse_term(&mut self) -> Result<f64, CalcError> {
        let mut left = self.parse_factor()?;

        while let Some(op) = self.peek() {
            match op {
                Token::Star => {
                    self.advance();
                    let right = self.parse_factor()?;
                    left *= right;
                }
                Token::Slash => {
                    self.advance();
                    let right = self.parse_factor()?;
                    if right == 0.0 {
                        return Err(CalcError::EvalError("division by zero".to_owned()));
                    }
                    left /= right;
                }
                _ => break,
            }
        }

        Ok(left)
    }

    fn parse_factor(&mut self) -> Result<f64, CalcError> {
        if let Some(Token::Plus) = self.peek() {
            self.advance();
            return self.parse_factor();
        }
        if let Some(Token::Minus) = self.peek() {
            self.advance();
            let val = self.parse_factor()?;
            return Ok(-val);
        }

        self.parse_primary()
    }

    fn parse_primary(&mut self) -> Result<f64, CalcError> {
        match self.advance().cloned() {
            Some(Token::Number(n)) => Ok(n),
            Some(Token::LParen) => {
                let val = self.parse_expr()?;
                match self.advance() {
                    Some(Token::RParen) => Ok(val),
                    _ => Err(CalcError::ParseError("expected ')'".to_owned())),
                }
            }
            Some(Token::Ident(name)) => {
                // Check if followed by '(' -> function call
                if let Some(Token::LParen) = self.peek() {
                    self.advance();
                    let arg = self.parse_expr()?;
                    match self.advance() {
                        Some(Token::RParen) => {}
                        _ => return Err(CalcError::ParseError("expected ')' after function argument".to_owned())),
                    }

                    match name.to_ascii_lowercase().as_str() {
                        "floor" => Ok(arg.floor()),
                        "ceil" => Ok(arg.ceil()),
                        "round" => Ok(arg.round()),
                        "abs" => Ok(arg.abs()),
                        _ => Err(CalcError::EvalError(format!("unknown function '{name}'"))),
                    }
                } else {
                    // Variable lookup
                    (self.lookup)(&name).map_err(CalcError::EvalError)
                }
            }
            _ => Err(CalcError::ParseError("unexpected token in primary expression".to_owned())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn test_calc_basic_arithmetic() {
        let vars = BTreeMap::from([
            ("balance", 100.0),
            ("price", 8.5),
            ("step-1.usdt", 50.0),
        ]);
        let lookup = |k: &str| vars.get(k).copied().ok_or_else(|| format!("not found: {k}"));

        assert_eq!(evaluate_calc("1 + 2 * 3", &lookup).unwrap(), 7.0);
        assert_eq!(evaluate_calc("(1 + 2) * 3", &lookup).unwrap(), 9.0);
        assert_eq!(evaluate_calc("floor(100 * 0.9 * 3 / 8.5)", &lookup).unwrap(), 31.0);
        assert_eq!(evaluate_calc("floor((step-1.usdt * 2) / price)", &lookup).unwrap(), 11.0);
    }
}
