#![allow(dead_code)]
/// Tokens produced by the OrimaLang lexer.
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Word(String),
    Number(f64),
    StringLit(String),
    Comma,
    Period,
    Eof,
}

/// A token together with the source line it came from.
#[derive(Debug, Clone)]
pub struct Spanned {
    pub token: Token,
    pub line: usize,
}

/// Tokenize an OrimaLang source string.
///
/// Rules:
/// - Whitespace (including newlines) separates tokens.
/// - `"..."` is a string literal token (may contain spaces).
/// - A bare `.` or `,` is emitted as `Period` / `Comma`.
/// - A word ending with `.` or `,` is split into word + punctuation token.
/// - A token that parses as an f64 becomes `Number`.
/// - Everything else becomes `Word` (lowercased so the language is case-insensitive).
/// - Lines starting with `note ` (after trimming) are treated as comments and skipped.
pub fn tokenize(source: &str) -> Vec<Spanned> {
    let mut tokens: Vec<Spanned> = Vec::new();

    for (line_idx, line) in source.lines().enumerate() {
        let line_no = line_idx + 1;

        // Skip comment lines (start with "note" case-insensitive)
        let trimmed = line.trim();
        if trimmed.to_lowercase().starts_with("note") {
            let rest = &trimmed[4..];
            if rest.is_empty() || rest.starts_with(' ') || rest.starts_with('\t') {
                continue;
            }
        }

        // Scan character by character
        let chars: Vec<char> = trimmed.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            // Skip whitespace
            if chars[i].is_whitespace() {
                i += 1;
                continue;
            }

            // String literal
            if chars[i] == '"' {
                i += 1; // skip opening quote
                let start = i;
                while i < chars.len() && chars[i] != '"' {
                    i += 1;
                }
                let content: String = chars[start..i].iter().collect();
                tokens.push(Spanned { token: Token::StringLit(content), line: line_no });
                if i < chars.len() {
                    i += 1; // skip closing quote
                }
                continue;
            }

            // Period
            if chars[i] == '.' {
                tokens.push(Spanned { token: Token::Period, line: line_no });
                i += 1;
                continue;
            }

            // Comma
            if chars[i] == ',' {
                tokens.push(Spanned { token: Token::Comma, line: line_no });
                i += 1;
                continue;
            }

            // Word / Number: read until whitespace, '.', ',', or '"'
            let start = i;
            while i < chars.len() && !chars[i].is_whitespace() && chars[i] != '.' && chars[i] != ',' && chars[i] != '"' {
                i += 1;
            }
            let raw: String = chars[start..i].iter().collect();
            if !raw.is_empty() {
                emit_core(&raw, line_no, &mut tokens);
            }
        }
    }

    tokens.push(Spanned { token: Token::Eof, line: source.lines().count().max(1) });
    tokens
}

fn emit_core(word: &str, line: usize, out: &mut Vec<Spanned>) {
    // Try numeric first
    if let Ok(n) = word.parse::<f64>() {
        out.push(Spanned { token: Token::Number(n), line });
        return;
    }
    // Lower-case so the language is case-insensitive
    out.push(Spanned { token: Token::Word(word.to_lowercase()), line });
}
