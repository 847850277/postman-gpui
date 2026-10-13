use super::{CurlError, Result};

/// Tokenize literal POSIX words; never evaluate expansions or execute a command.
pub(super) fn words(input: &str) -> Result<Vec<String>> {
    if input.len() > 256 * 1024 {
        return Err(CurlError::new("Keep the cURL command below 256 KiB."));
    }
    if input.contains('\0') {
        return Err(CurlError::new("A command cannot contain a null character."));
    }
    let mut chars = input.chars().peekable();
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quote = None;
    let mut started = false;
    while let Some(c) = chars.next() {
        if quote == Some('\'') {
            if c == '\'' {
                quote = None;
            } else {
                word.push(c);
            }
            continue;
        }
        match c {
            '"' if quote == Some('"') => quote = None,
            '\'' | '"' if quote.is_none() => { quote = Some(c); started = true; }
            '\\' => {
                let next = chars.next().ok_or_else(|| CurlError::new("Complete the trailing backslash escape."))?;
                if next == '\n' { continue; }
                if next == '\r' && chars.peek() == Some(&'\n') { chars.next(); continue; }
                if quote == Some('"') && !matches!(next, '"' | '\\' | '$' | '`') { word.push('\\'); }
                word.push(next);
                started = true;
            }
            '$' | '`' => return Err(CurlError::new("Paste resolved values. Shell variables and command substitution are not supported; quote literal characters with single quotes.")),
            '|' | '&' | ';' | '<' | '>' | '(' | ')' if quote.is_none() => return Err(CurlError::new("Paste a single cURL command without pipes, redirection or command chaining.")),
            '*' | '[' | ']' | '{' | '}' | '~' if quote.is_none() => return Err(CurlError::new("Quote literal shell glob and tilde characters.")),
            '\n' | '\r' if quote.is_none() => {
                if words.is_empty() && !started { continue; }
                if chars.clone().all(|c| matches!(c, ' ' | '\t' | '\n' | '\r')) { break; }
                return Err(CurlError::new("Join command lines with a backslash continuation, or paste one single-line command."));
            }
            ' ' | '\t' if quote.is_none() => {
                if started { words.push(std::mem::take(&mut word)); started = false; }
            }
            _ => { word.push(c); started = true; }
        }
    }
    if quote.is_some() {
        return Err(CurlError::new("Close the unfinished quotation."));
    }
    if started {
        words.push(word);
    }
    Ok(words)
}

pub(super) fn quote(value: &str) -> Result<String> {
    if value.contains('\0') {
        return Err(CurlError::new(
            "POSIX command arguments cannot contain a null character.",
        ));
    }
    Ok(format!("'{}'", value.replace('\'', "'\\''")))
}
