#![allow(dead_code)]
use std::collections::HashMap;
use crate::lexer::{Spanned, Token};

// ─── Value type (mirrors evaluator, kept here for Literal) ───────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Str(String),
    Num(f64),
    Bool(bool),
    List(Vec<Value>),
    Object(String, HashMap<String, Value>),
    Nil,
}

// ─── Expression AST ──────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum Comparison {
    Is,
    IsNot,
    GreaterThan,
    LessThan,
    AtLeast,
    AtMost,
    Contains,
    StartsWith,
    EndsWith,
}

#[derive(Debug, Clone)]
pub enum Op {
    Plus,
    Minus,
    Times,
    DividedBy,
    Remainder,
}

#[derive(Debug, Clone)]
pub enum LogicOp {
    And,
    Or,
}

#[derive(Debug, Clone)]
pub enum Expr {
    Literal(Value),
    Variable(String),
    BinaryOp(Box<Expr>, Op, Box<Expr>),
    Condition(Box<Expr>, Comparison, Box<Expr>),
    LogicalOp(Box<Expr>, LogicOp, Box<Expr>),
    Not(Box<Expr>),
    ItemFrom(Box<Expr>, String),
    SizeOf(String),
    LengthOf(String),
    JoinedWith(Box<Expr>, Box<Expr>),
    InUppercase(Box<Expr>),
    InLowercase(Box<Expr>),
    // Concatenation list (for `say` with `and` separators)
    Concat(Vec<Expr>),
    // Object field access: `the <field> of <objvar>`
    GetField(String, String),
}

// ─── Statement AST ───────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum Stmt {
    Say(Vec<Expr>),
    Set(String, Expr),
    Increase(String, Expr),
    Decrease(String, Expr),
    Multiply(String, Expr),
    Divide(String, Expr),
    If(Expr, Vec<Stmt>, Vec<Stmt>),
    Repeat(Expr, Vec<Stmt>),
    While(Expr, Vec<Stmt>),
    ForEach(String, String, Vec<Stmt>),
    StopLoop,
    SkipToNext,
    CreateList(String),
    AddToList(Expr, String),
    RemoveFromList(Expr, String),
    RemoveItemFromList(Expr, String),
    Define(String, Vec<String>, Vec<Stmt>),
    Run(String, Vec<Expr>, Option<String>),
    GiveBack(Expr),
    Ask(Vec<String>, String),
    // Classes
    DefineClass(String, Vec<String>, Vec<(String, Vec<String>, Vec<Stmt>)>),
    // class_name, field_names, methods=(method_name, params, body)
    CreateObject(String, Vec<Expr>, String),
    // class_name, constructor_args, store_var
    SetField(String, String, Expr),
    // obj_var, field_name, value_expr
    RunMethod(String, Vec<Expr>, String, Option<String>),
    // method_name, args, obj_var, store_var
}

// ─── Parser ──────────────────────────────────────────────────────────────────

pub struct Parser {
    tokens: Vec<Spanned>,
    pos: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Spanned>) -> Self {
        Parser { tokens, pos: 0 }
    }

    // ── Low-level helpers ──────────────────────────────────────────────────

    fn peek(&self) -> &Spanned {
        &self.tokens[self.pos]
    }

    fn advance(&mut self) -> &Spanned {
        let t = &self.tokens[self.pos];
        if self.pos + 1 < self.tokens.len() {
            self.pos += 1;
        }
        t
    }

    fn current_line(&self) -> usize {
        self.peek().line
    }

    fn is_eof(&self) -> bool {
        matches!(self.peek().token, Token::Eof)
    }

    // ── Statement-level parsing ────────────────────────────────────────────

    /// Parse all statements from the token stream.
    pub fn parse_program(&mut self) -> Result<Vec<Stmt>, String> {
        let mut stmts = Vec::new();
        while !self.is_eof() {
            // Collect tokens until we hit a Period (end of statement)
            let stmt_tokens = self.collect_statement()?;
            if stmt_tokens.is_empty() {
                continue;
            }
            let parsed = self.parse_statement_tokens(&stmt_tokens)?;
            stmts.extend(parsed);
        }
        Ok(stmts)
    }

    /// Collect all tokens up to and including the next Period,
    /// but skip Periods that are "inside" block structures.
    /// Returns the tokens WITHOUT the terminating Period.
    fn collect_statement(&mut self) -> Result<Vec<Spanned>, String> {
        let mut result = Vec::new();
        loop {
            let t = self.peek().clone();
            match &t.token {
                Token::Eof => break,
                Token::Period => {
                    self.advance();
                    break;
                }
                _ => {
                    result.push(t);
                    self.advance();
                }
            }
        }
        Ok(result)
    }

    /// Split a flat token slice on Comma tokens, yielding clause slices.
    fn split_clauses(tokens: &[Spanned]) -> Vec<Vec<Spanned>> {
        let mut clauses: Vec<Vec<Spanned>> = Vec::new();
        let mut current: Vec<Spanned> = Vec::new();
        for t in tokens {
            if matches!(t.token, Token::Comma) {
                clauses.push(current.clone());
                current.clear();
            } else {
                current.push(t.clone());
            }
        }
        if !current.is_empty() {
            clauses.push(current);
        }
        clauses
    }

    /// Given the tokens of one full statement (no trailing Period), parse it.
    /// A single "statement" in OrimaLang can contain block structures like
    /// `if … , body …, end if` – we handle those by looking for end markers.
    fn parse_statement_tokens(&self, tokens: &[Spanned]) -> Result<Vec<Stmt>, String> {
        let clauses = Self::split_clauses(tokens);
        if clauses.is_empty() {
            return Ok(vec![]);
        }
        self.parse_clauses(&clauses, 0).map(|(stmts, _)| stmts)
    }

    /// Recursively parse a sequence of clauses starting at `idx`.
    /// Returns (statements, next_idx_after_consumed_clauses).
    fn parse_clauses(&self, clauses: &[Vec<Spanned>], idx: usize) -> Result<(Vec<Stmt>, usize), String> {
        let mut stmts = Vec::new();
        let mut i = idx;
        while i < clauses.len() {
            // Check for end markers: "end if", "end repeat", "end while", "end for", "end define", "end class"
            // These terminate the current block – the caller handles them.
            if Self::is_end_marker(&clauses[i]) {
                break;
            }
            // Check for "otherwise" – also terminates the if-body
            if Self::clause_starts_with(&clauses[i], "otherwise") {
                break;
            }

            let (stmt, consumed) = self.parse_one_clause(clauses, i)?;
            i = consumed;
            stmts.push(stmt);
        }
        Ok((stmts, i))
    }

    fn is_end_marker(clause: &[Spanned]) -> bool {
        if clause.is_empty() {
            return false;
        }
        if let Token::Word(w) = &clause[0].token {
            if w == "end" {
                return true;
            }
        }
        false
    }

    fn clause_starts_with(clause: &[Spanned], keyword: &str) -> bool {
        if let Some(first) = clause.first() {
            if let Token::Word(w) = &first.token {
                return w == keyword;
            }
        }
        false
    }

    fn words(clause: &[Spanned]) -> Vec<String> {
        clause.iter().filter_map(|t| {
            if let Token::Word(w) = &t.token { Some(w.clone()) } else { None }
        }).collect()
    }

    fn line_of(clause: &[Spanned]) -> usize {
        clause.first().map(|t| t.line).unwrap_or(1)
    }

    // ── Single-clause / block parsing ─────────────────────────────────────

    fn parse_one_clause(&self, clauses: &[Vec<Spanned>], idx: usize) -> Result<(Stmt, usize), String> {
        let clause = &clauses[idx];
        if clause.is_empty() {
            // Empty clause – shouldn't normally happen, skip
            return Ok((Stmt::Say(vec![]), idx + 1));
        }
        let line = Self::line_of(clause);
        let first_word = match &clause[0].token {
            Token::Word(w) => w.clone(),
            Token::Number(_) => {
                return Err(format!("OrimaLang Error: unexpected number at start of statement on line {line}"));
            }
            _ => {
                return Err(format!("OrimaLang Error: unexpected token at start of statement on line {line}"));
            }
        };

        match first_word.as_str() {
            "say" => {
                let stmt = self.parse_say(clause, line)?;
                Ok((stmt, idx + 1))
            }
            "set" => {
                let stmt = self.parse_set(clause, line)?;
                Ok((stmt, idx + 1))
            }
            "increase" => {
                let stmt = self.parse_arith("increase", clause, line)?;
                Ok((stmt, idx + 1))
            }
            "decrease" => {
                let stmt = self.parse_arith("decrease", clause, line)?;
                Ok((stmt, idx + 1))
            }
            "multiply" => {
                let stmt = self.parse_arith("multiply", clause, line)?;
                Ok((stmt, idx + 1))
            }
            "divide" => {
                let stmt = self.parse_arith("divide", clause, line)?;
                Ok((stmt, idx + 1))
            }
            "if" => {
                self.parse_if(clauses, idx, line)
            }
            "repeat" => {
                self.parse_repeat(clauses, idx, line)
            }
            "while" => {
                self.parse_while(clauses, idx, line)
            }
            "for" => {
                self.parse_for_each(clauses, idx, line)
            }
            "stop" => {
                Ok((Stmt::StopLoop, idx + 1))
            }
            "skip" => {
                Ok((Stmt::SkipToNext, idx + 1))
            }
            "create" => {
                let words = Self::words(clause);
                if words.len() >= 2 && words[1] == "list" {
                    let stmt = self.parse_create_list(clause, line)?;
                    Ok((stmt, idx + 1))
                } else {
                    let stmt = self.parse_create_object(clause, line)?;
                    Ok((stmt, idx + 1))
                }
            }
            "add" => {
                let stmt = self.parse_add_to_list(clause, line)?;
                Ok((stmt, idx + 1))
            }
            "remove" => {
                let stmt = self.parse_remove(clause, line)?;
                Ok((stmt, idx + 1))
            }
            "define" => {
                let words = Self::words(clause);
                if words.len() >= 2 && words[1] == "type" {
                    self.parse_define_class(clauses, idx, line)
                } else {
                    self.parse_define(clauses, idx, line)
                }
            }
            "run" => {
                // Check if "on" keyword exists — method call
                let on_pos = clause.iter().position(|t| Self::tok_is_word(t, "on"));
                if on_pos.is_some() {
                    let stmt = self.parse_run_method(clause, line)?;
                    Ok((stmt, idx + 1))
                } else {
                    let stmt = self.parse_run(clause, line)?;
                    Ok((stmt, idx + 1))
                }
            }
            "give" => {
                let stmt = self.parse_give_back(clause, line)?;
                Ok((stmt, idx + 1))
            }
            "ask" => {
                let stmt = self.parse_ask(clause, line)?;
                Ok((stmt, idx + 1))
            }
            other => {
                Err(format!("OrimaLang Error: unknown statement keyword '{other}' on line {line}"))
            }
        }
    }

    // ── say ───────────────────────────────────────────────────────────────

    /// `say hello and name and nice to meet you`
    /// Splits on "and" to produce concatenation parts.
    fn parse_say(&self, clause: &[Spanned], line: usize) -> Result<Stmt, String> {
        // tokens after "say"
        let rest = &clause[1..];
        if rest.is_empty() {
            return Ok(Stmt::Say(vec![Expr::Literal(Value::Str(String::new()))]));
        }
        let parts = self.parse_say_parts(rest, line)?;
        Ok(Stmt::Say(parts))
    }

    fn parse_say_parts(&self, tokens: &[Spanned], line: usize) -> Result<Vec<Expr>, String> {
        // Split on bare "and" tokens to get parts
        let segments = Self::split_on_and(tokens);
        let mut exprs = Vec::new();
        for seg in segments {
            if seg.is_empty() {
                continue;
            }
            let expr = self.parse_value_expr(&seg, line)?;
            exprs.push(expr);
        }
        if exprs.is_empty() {
            exprs.push(Expr::Literal(Value::Str(String::new())));
        }
        Ok(exprs)
    }

    fn split_on_and(tokens: &[Spanned]) -> Vec<Vec<Spanned>> {
        let mut segments: Vec<Vec<Spanned>> = Vec::new();
        let mut current: Vec<Spanned> = Vec::new();
        let mut i = 0;
        while i < tokens.len() {
            if let Token::Word(w) = &tokens[i].token {
                if w == "and" {
                    segments.push(current.clone());
                    current.clear();
                    i += 1;
                    continue;
                }
            }
            current.push(tokens[i].clone());
            i += 1;
        }
        if !current.is_empty() {
            segments.push(current);
        }
        segments
    }

    // ── set ───────────────────────────────────────────────────────────────

    /// `set <name> to <expr>`, `set <field> of <obj> to <expr>`, or `set the <field> of <obj> to <expr>`
    fn parse_set(&self, clause: &[Spanned], line: usize) -> Result<Stmt, String> {
        // Detect `set <field> of <obj> to <expr>` or `set the <field> of <obj> to <expr>`
        // Look for "of" followed later by "to" — that means it's a field assignment
        let of_pos = clause.iter().position(|t| Self::tok_is_word(t, "of"));
        let to_pos = clause.iter().rposition(|t| Self::tok_is_word(t, "to"));
        if let (Some(of_p), Some(to_p)) = (of_pos, to_pos) {
            if of_p > 1 && to_p > of_p {
                // field name: words between "set" (skip "the" if present) and "of"
                let field_start = if Self::tok_is_word(&clause[1], "the") { 2 } else { 1 };
                let field_name = clause[field_start..of_p].iter()
                    .filter_map(|t| if let Token::Word(w) = &t.token { Some(w.clone()) } else { None })
                    .collect::<Vec<_>>()
                    .join(" ");
                let obj_name = match clause.get(of_p + 1) {
                    Some(t) => match &t.token {
                        Token::Word(w) => w.clone(),
                        _ => return Err(format!("OrimaLang Error: expected object variable name after 'of' on line {line}")),
                    },
                    None => return Err(format!("OrimaLang Error: expected object variable name after 'of' on line {line}")),
                };
                if !field_name.is_empty() {
                    let val_tokens = &clause[to_p + 1..];
                    let val_expr = self.parse_expr(val_tokens, line)?;
                    return Ok(Stmt::SetField(obj_name, field_name, val_expr));
                }
            }
        }

        // Detect `set the <field> of <obj> to <expr>` (legacy — now handled above, kept for clarity)
        if clause.len() >= 2 && Self::tok_is_word(&clause[1], "the") {
            // Find "of" position
            let of_pos = clause.iter().position(|t| Self::tok_is_word(t, "of"));
            // Find "to" position (last occurrence)
            let to_pos = clause.iter().rposition(|t| Self::tok_is_word(t, "to"));
            if let (Some(of_p), Some(to_p)) = (of_pos, to_pos) {
                if of_p > 1 && to_p > of_p {
                    // field name = words between positions 2..of_p
                    let field_name = clause[2..of_p].iter()
                        .filter_map(|t| if let Token::Word(w) = &t.token { Some(w.clone()) } else { None })
                        .collect::<Vec<_>>()
                        .join(" ");
                    // obj name = word at of_p + 1
                    let obj_name = match clause.get(of_p + 1) {
                        Some(t) => match &t.token {
                            Token::Word(w) => w.clone(),
                            _ => return Err(format!("OrimaLang Error: expected object variable name after 'of' on line {line}")),
                        },
                        None => return Err(format!("OrimaLang Error: expected object variable name after 'of' on line {line}")),
                    };
                    // value = tokens after to_pos
                    let val_tokens = &clause[to_p + 1..];
                    let val_expr = self.parse_expr(val_tokens, line)?;
                    return Ok(Stmt::SetField(obj_name, field_name, val_expr));
                }
            }
        }

        // Standard `set <name> to <expr>`
        if clause.len() < 4 {
            return Err(format!("OrimaLang Error: malformed 'set' statement on line {line}"));
        }
        let name = match &clause[1].token {
            Token::Word(w) => w.clone(),
            _ => return Err(format!("OrimaLang Error: expected variable name after 'set' on line {line}")),
        };
        if !matches!(&clause[2].token, Token::Word(w) if w == "to") {
            return Err(format!("OrimaLang Error: expected 'to' in 'set' statement on line {line}"));
        }
        let rest = &clause[3..];
        let expr = self.parse_expr(rest, line)?;
        Ok(Stmt::Set(name, expr))
    }

    // ── arithmetic assignment ─────────────────────────────────────────────

    /// increase <name> by <expr>
    /// decrease <name> by <expr>
    /// multiply <name> by <expr>
    /// divide <name> by <expr>
    fn parse_arith(&self, kind: &str, clause: &[Spanned], line: usize) -> Result<Stmt, String> {
        // [kind, name, "by", expr...]
        if clause.len() < 4 {
            return Err(format!("OrimaLang Error: malformed '{kind}' statement on line {line}"));
        }
        let name = match &clause[1].token {
            Token::Word(w) => w.clone(),
            _ => return Err(format!("OrimaLang Error: expected variable name in '{kind}' on line {line}")),
        };
        // expect "by"
        if !matches!(&clause[2].token, Token::Word(w) if w == "by") {
            return Err(format!("OrimaLang Error: expected 'by' in '{kind}' statement on line {line}"));
        }
        let rest = &clause[3..];
        let expr = self.parse_expr(rest, line)?;
        match kind {
            "increase" => Ok(Stmt::Increase(name, expr)),
            "decrease" => Ok(Stmt::Decrease(name, expr)),
            "multiply" => Ok(Stmt::Multiply(name, expr)),
            "divide" => Ok(Stmt::Divide(name, expr)),
            _ => unreachable!(),
        }
    }

    // ── if ────────────────────────────────────────────────────────────────

    fn parse_if(&self, clauses: &[Vec<Spanned>], idx: usize, line: usize) -> Result<(Stmt, usize), String> {
        let clause = &clauses[idx];
        // `if <condition>` – rest of clause is the condition
        let cond_tokens = &clause[1..];
        let cond = self.parse_condition_expr(cond_tokens, line)?;

        // Parse then-body clauses until "otherwise" or "end if"
        let (then_body, mut next) = self.parse_clauses(clauses, idx + 1)?;

        // Check for "otherwise"
        let mut else_body = Vec::new();
        if next < clauses.len() && Self::clause_starts_with(&clauses[next], "otherwise") {
            // consume "otherwise" clause (it may itself contain statements after the keyword)
            let ow_clause = &clauses[next];
            let after_otherwise = &ow_clause[1..]; // tokens after "otherwise"
            next += 1;
            // If there are tokens after "otherwise" in the same clause, parse them as a single statement
            if !after_otherwise.is_empty() {
                let (s, _) = self.parse_one_clause_tokens(after_otherwise, line)?;
                else_body.push(s);
            } else {
                let (body, nx) = self.parse_clauses(clauses, next)?;
                else_body = body;
                next = nx;
            }
        }

        // Consume "end if"
        if next < clauses.len() && Self::is_end_marker(&clauses[next]) {
            next += 1;
        }

        Ok((Stmt::If(cond, then_body, else_body), next))
    }

    /// Parse a single statement from a raw token slice (not split into clauses).
    fn parse_one_clause_tokens(&self, tokens: &[Spanned], line: usize) -> Result<(Stmt, usize), String> {
        // Wrap in single-element clause array
        let clauses = vec![tokens.to_vec()];
        let (stmts, next) = self.parse_clauses(&clauses, 0)?;
        let stmt = stmts.into_iter().next().unwrap_or(Stmt::Say(vec![]));
        Ok((stmt, next))
    }

    // ── repeat ────────────────────────────────────────────────────────────

    fn parse_repeat(&self, clauses: &[Vec<Spanned>], idx: usize, line: usize) -> Result<(Stmt, usize), String> {
        // `repeat <n> times`
        let clause = &clauses[idx];
        // find "times"
        let times_pos = clause.iter().position(|t| matches!(&t.token, Token::Word(w) if w == "times"));
        let count_tokens = if let Some(p) = times_pos {
            &clause[1..p]
        } else {
            &clause[1..]
        };
        let count_expr = self.parse_expr(count_tokens, line)?;
        let (body, mut next) = self.parse_clauses(clauses, idx + 1)?;
        if next < clauses.len() && Self::is_end_marker(&clauses[next]) {
            next += 1;
        }
        Ok((Stmt::Repeat(count_expr, body), next))
    }

    // ── while ─────────────────────────────────────────────────────────────

    fn parse_while(&self, clauses: &[Vec<Spanned>], idx: usize, line: usize) -> Result<(Stmt, usize), String> {
        let clause = &clauses[idx];
        let cond_tokens = &clause[1..];
        let cond = self.parse_condition_expr(cond_tokens, line)?;
        let (body, mut next) = self.parse_clauses(clauses, idx + 1)?;
        if next < clauses.len() && Self::is_end_marker(&clauses[next]) {
            next += 1;
        }
        Ok((Stmt::While(cond, body), next))
    }

    // ── for each ──────────────────────────────────────────────────────────

    fn parse_for_each(&self, clauses: &[Vec<Spanned>], idx: usize, line: usize) -> Result<(Stmt, usize), String> {
        // `for each <item> in <list>`
        let clause = &clauses[idx];
        let words = Self::words(clause);
        // words[0]=for words[1]=each words[2]=item words[3]=in words[4]=list
        if words.len() < 5 || words[1] != "each" || words[3] != "in" {
            return Err(format!("OrimaLang Error: malformed 'for each' statement on line {line}"));
        }
        let item_var = words[2].clone();
        let list_var = words[4].clone();
        let (body, mut next) = self.parse_clauses(clauses, idx + 1)?;
        if next < clauses.len() && Self::is_end_marker(&clauses[next]) {
            next += 1;
        }
        Ok((Stmt::ForEach(item_var, list_var, body), next))
    }

    // ── create list ───────────────────────────────────────────────────────

    fn parse_create_list(&self, clause: &[Spanned], line: usize) -> Result<Stmt, String> {
        // `create list <name>`
        let words = Self::words(clause);
        if words.len() < 3 || words[1] != "list" {
            return Err(format!("OrimaLang Error: malformed 'create list' on line {line}"));
        }
        Ok(Stmt::CreateList(words[2].clone()))
    }

    // ── create object ─────────────────────────────────────────────────────

    fn parse_create_object(&self, clause: &[Spanned], line: usize) -> Result<Stmt, String> {
        // `create <ClassName> with <args> and store in <var>`
        // class name = words between "create" and "with"/"store"/end
        // args = segments between "with" and "and store"
        // store_var = word after "in"

        let with_pos = clause.iter().position(|t| Self::tok_is_word(t, "with"));
        let store_pos = clause.iter().position(|t| Self::tok_is_word(t, "store"));

        // Class name end position
        let name_end = with_pos.or(store_pos).unwrap_or(clause.len());
        let class_name = clause[1..name_end].iter()
            .filter_map(|t| if let Token::Word(w) = &t.token { Some(w.clone()) } else { None })
            .collect::<Vec<_>>()
            .join(" ");

        if class_name.is_empty() {
            return Err(format!("OrimaLang Error: 'create' needs a class name on line {line}"));
        }

        // Store variable
        let store_var = if let Some(sp) = store_pos {
            let in_pos = clause.iter().position(|t| Self::tok_is_word(t, "in"));
            if let Some(ip) = in_pos {
                if ip == sp + 1 {
                    clause.get(ip + 1).and_then(|t| {
                        if let Token::Word(w) = &t.token { Some(w.clone()) } else { None }
                    })
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        let store_var = store_var.ok_or_else(|| format!("OrimaLang Error: 'create' needs 'store in <var>' on line {line}"))?;

        // Args between "with" and "and store"
        let args = if let Some(wp) = with_pos {
            let end_pos = store_pos
                .map(|sp| {
                    if sp > 0 && matches!(&clause[sp - 1].token, Token::Word(w) if w == "and") {
                        sp - 1
                    } else {
                        sp
                    }
                })
                .unwrap_or(clause.len());
            let arg_tokens = &clause[wp + 1..end_pos];
            let segs = Self::split_on_and(arg_tokens);
            let mut exprs = Vec::new();
            for seg in segs {
                if !seg.is_empty() {
                    let e = self.parse_value_expr(&seg, line)?;
                    exprs.push(e);
                }
            }
            exprs
        } else {
            vec![]
        };

        Ok(Stmt::CreateObject(class_name, args, store_var))
    }

    // ── add to list ───────────────────────────────────────────────────────

    fn parse_add_to_list(&self, clause: &[Spanned], line: usize) -> Result<Stmt, String> {
        // `add <expr> to <listname>`
        // find "to" keyword
        let to_pos = clause.iter().rposition(|t| matches!(&t.token, Token::Word(w) if w == "to"));
        let to_pos = to_pos.ok_or_else(|| format!("OrimaLang Error: 'add' missing 'to' on line {line}"))?;
        let val_tokens = &clause[1..to_pos];
        let list_name = match clause.get(to_pos + 1) {
            Some(t) => match &t.token {
                Token::Word(w) => w.clone(),
                _ => return Err(format!("OrimaLang Error: expected list name after 'to' on line {line}")),
            },
            None => return Err(format!("OrimaLang Error: missing list name in 'add' on line {line}")),
        };
        let val_expr = self.parse_value_expr(val_tokens, line)?;
        Ok(Stmt::AddToList(val_expr, list_name))
    }

    // ── remove ────────────────────────────────────────────────────────────

    fn parse_remove(&self, clause: &[Spanned], line: usize) -> Result<Stmt, String> {
        // `remove <value> from <list>` or `remove item <n> from <list>`
        // Find "from" token position in the raw clause (not words-only)
        let from_tok_pos = clause.iter().position(|t| matches!(&t.token, Token::Word(w) if w == "from"))
            .ok_or_else(|| format!("OrimaLang Error: 'remove' missing 'from' on line {line}"))?;
        let list_name = match clause.get(from_tok_pos + 1) {
            Some(t) => match &t.token {
                Token::Word(w) => w.clone(),
                _ => return Err(format!("OrimaLang Error: expected list name after 'from' on line {line}")),
            },
            None => return Err(format!("OrimaLang Error: missing list name in 'remove' on line {line}")),
        };

        // `remove item <n> from ...` -> RemoveItemFromList
        let is_item = clause.get(1).map(|t| Self::tok_is_word(t, "item")).unwrap_or(false);
        if is_item {
            // index tokens between "item" (pos 1) and "from" (from_tok_pos)
            let idx_tokens: Vec<Spanned> = clause[2..from_tok_pos].to_vec();
            let idx_expr = self.parse_expr(&idx_tokens, line)?;
            return Ok(Stmt::RemoveItemFromList(idx_expr, list_name));
        }

        // Otherwise: `remove <value> from <list>`
        let val_tokens: Vec<Spanned> = clause[1..from_tok_pos].to_vec();
        let val_expr = self.parse_value_expr(&val_tokens, line)?;
        Ok(Stmt::RemoveFromList(val_expr, list_name))
    }

    // ── define ────────────────────────────────────────────────────────────

    fn parse_define(&self, clauses: &[Vec<Spanned>], idx: usize, line: usize) -> Result<(Stmt, usize), String> {
        // `define <name> taking <p1> and <p2> ...`
        // or `define <name>` (no params)
        let clause = &clauses[idx];
        let words = Self::words(clause);
        if words.len() < 2 {
            return Err(format!("OrimaLang Error: 'define' needs a function name on line {line}"));
        }
        let mut params = Vec::new();
        let taking_pos = words.iter().position(|w| w == "taking");
        // Function name: words[1..taking_pos) or words[1..end)
        let name_end = taking_pos.unwrap_or(words.len());
        let func_name: String = words[1..name_end].join(" ");
        if func_name.is_empty() {
            return Err(format!("OrimaLang Error: 'define' needs a function name on line {line}"));
        }
        if let Some(tp) = taking_pos {
            // params are words after "taking", separated by "and"
            let param_words = &words[tp + 1..];
            for w in param_words {
                if w != "and" {
                    params.push(w.clone());
                }
            }
        }
        let (body, mut next) = self.parse_clauses(clauses, idx + 1)?;
        if next < clauses.len() && Self::is_end_marker(&clauses[next]) {
            next += 1;
        }
        Ok((Stmt::Define(func_name, params, body), next))
    }

    // ── define type ───────────────────────────────────────────────────────

    fn parse_define_class(&self, clauses: &[Vec<Spanned>], idx: usize, line: usize) -> Result<(Stmt, usize), String> {
        // `define type <Name> taking <f1> and <f2> ...`
        let clause = &clauses[idx];
        let words = Self::words(clause);
        // words[0]=define words[1]=type words[2]=TypeName [words[3]=taking ...]
        if words.len() < 3 {
            return Err(format!("OrimaLang Error: 'define type' needs a type name on line {line}"));
        }
        let class_name = words[2].clone();

        // Collect field names (after "taking", separated by "and")
        let mut fields = Vec::new();
        let taking_pos = words.iter().position(|w| w == "taking");
        if let Some(tp) = taking_pos {
            for w in &words[tp + 1..] {
                if w != "and" {
                    fields.push(w.clone());
                }
            }
        }

        // Parse method definitions in the body
        let mut methods: Vec<(String, Vec<String>, Vec<Stmt>)> = Vec::new();
        let mut i = idx + 1;
        while i < clauses.len() {
            // Stop at "end class"
            if Self::is_end_marker(&clauses[i]) {
                let end_words = Self::words(&clauses[i]);
                if end_words.len() >= 2 && end_words[1] == "type" {
                    i += 1; // consume "end type"
                    break;
                }
                // Some other end marker — let outer context handle
                break;
            }
            if Self::clause_starts_with(&clauses[i], "define") {
                let clause_line = Self::line_of(&clauses[i]);
                let (stmt, next) = self.parse_define(clauses, i, clause_line)?;
                if let Stmt::Define(name, params, body) = stmt {
                    methods.push((name, params, body));
                }
                i = next;
            } else {
                // Skip unknown clauses in class body (or error)
                i += 1;
            }
        }

        Ok((Stmt::DefineClass(class_name, fields, methods), i))
    }

    // ── run ───────────────────────────────────────────────────────────────

    fn parse_run(&self, clause: &[Spanned], line: usize) -> Result<Stmt, String> {
        // `run <name> with <arg1> and <arg2> and store in <var>`
        // `run <name>` (no args)
        // Function name is all words between "run" and "with" (or end of clause).
        // We work on the raw token array (clause) to handle numbers in args.

        if clause.len() < 2 {
            return Err(format!("OrimaLang Error: 'run' needs a function name on line {line}"));
        }

        // Find "with" token position in clause
        let with_tok_pos = clause.iter().position(|t| matches!(&t.token, Token::Word(w) if w == "with"));

        // Find "store" token position
        let store_tok_pos = clause.iter().position(|t| matches!(&t.token, Token::Word(w) if w == "store"));

        // Function name: tokens [1..with_tok_pos) or [1..store_tok_pos) or [1..end)
        // Only Word tokens contribute to the function name
        let name_end = with_tok_pos
            .or(store_tok_pos)
            .unwrap_or(clause.len());
        let func_name: String = clause[1..name_end].iter()
            .filter_map(|t| if let Token::Word(w) = &t.token { Some(w.clone()) } else { None })
            .collect::<Vec<_>>()
            .join(" ");

        if func_name.is_empty() {
            return Err(format!("OrimaLang Error: 'run' needs a function name on line {line}"));
        }

        // Store var: if "store in <var>" appears
        let store_var = if let Some(sp) = store_tok_pos {
            // Check next word is "in"
            let in_pos = clause.iter().position(|t| matches!(&t.token, Token::Word(w) if w == "in"));
            if let Some(ip) = in_pos {
                if ip == sp + 1 {
                    // var name is next word after "in"
                    clause.get(ip + 1).and_then(|t| {
                        if let Token::Word(w) = &t.token { Some(w.clone()) } else { None }
                    })
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        // Args: tokens between "with" and "store" (or end)
        let args = if let Some(wp) = with_tok_pos {
            let end_tok_pos = store_tok_pos
                // "and store in" — go back one more for the "and"
                .map(|sp| {
                    if sp > 0 && matches!(&clause[sp - 1].token, Token::Word(w) if w == "and") {
                        sp - 1
                    } else {
                        sp
                    }
                })
                .unwrap_or(clause.len());
            let arg_tokens = &clause[wp + 1..end_tok_pos];
            // split on "and"
            let segs = Self::split_on_and(arg_tokens);
            let mut exprs = Vec::new();
            for seg in segs {
                if !seg.is_empty() {
                    let e = self.parse_value_expr(&seg, line)?;
                    exprs.push(e);
                }
            }
            exprs
        } else {
            vec![]
        };

        Ok(Stmt::Run(func_name, args, store_var))
    }

    // ── run method ────────────────────────────────────────────────────────

    fn parse_run_method(&self, clause: &[Spanned], line: usize) -> Result<Stmt, String> {
        // `run <method> on <obj>` or
        // `run <method> on <obj> with <args>` or
        // `run <method> on <obj> and store in <result>` or
        // `run <method> on <obj> with <args> and store in <result>`

        let on_pos = clause.iter().position(|t| Self::tok_is_word(t, "on"))
            .ok_or_else(|| format!("OrimaLang Error: 'run ... on' missing 'on' on line {line}"))?;

        // Method name: words between "run" (pos 0) and "on"
        let method_name = clause[1..on_pos].iter()
            .filter_map(|t| if let Token::Word(w) = &t.token { Some(w.clone()) } else { None })
            .collect::<Vec<_>>()
            .join(" ");

        if method_name.is_empty() {
            return Err(format!("OrimaLang Error: 'run on' needs a method name on line {line}"));
        }

        // Object var: word immediately after "on"
        let obj_var = match clause.get(on_pos + 1) {
            Some(t) => match &t.token {
                Token::Word(w) => w.clone(),
                _ => return Err(format!("OrimaLang Error: expected object variable after 'on' on line {line}")),
            },
            None => return Err(format!("OrimaLang Error: expected object variable after 'on' on line {line}")),
        };

        // Find "with" and "store" after on_pos + 1
        let with_pos = clause[on_pos + 2..].iter().position(|t| Self::tok_is_word(t, "with"))
            .map(|p| p + on_pos + 2);
        let store_pos = clause[on_pos + 2..].iter().position(|t| Self::tok_is_word(t, "store"))
            .map(|p| p + on_pos + 2);

        // Store variable
        let store_var = if let Some(sp) = store_pos {
            let in_pos = clause[sp..].iter().position(|t| Self::tok_is_word(t, "in"))
                .map(|p| p + sp);
            if let Some(ip) = in_pos {
                if ip == sp + 1 {
                    clause.get(ip + 1).and_then(|t| {
                        if let Token::Word(w) = &t.token { Some(w.clone()) } else { None }
                    })
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        // Args between "with" and "store" (or end)
        let args = if let Some(wp) = with_pos {
            let end_pos = store_pos
                .map(|sp| {
                    if sp > 0 && matches!(&clause[sp - 1].token, Token::Word(w) if w == "and") {
                        sp - 1
                    } else {
                        sp
                    }
                })
                .unwrap_or(clause.len());
            let arg_tokens = &clause[wp + 1..end_pos];
            let segs = Self::split_on_and(arg_tokens);
            let mut exprs = Vec::new();
            for seg in segs {
                if !seg.is_empty() {
                    let e = self.parse_value_expr(&seg, line)?;
                    exprs.push(e);
                }
            }
            exprs
        } else {
            vec![]
        };

        Ok(Stmt::RunMethod(method_name, args, obj_var, store_var))
    }

    // ── give back ─────────────────────────────────────────────────────────

    fn parse_give_back(&self, clause: &[Spanned], line: usize) -> Result<Stmt, String> {
        // `give back <expr>`
        let words = Self::words(clause);
        if words.len() < 2 || words[1] != "back" {
            return Err(format!("OrimaLang Error: expected 'give back <value>' on line {line}"));
        }
        let rest = &clause[2..];
        let expr = self.parse_expr(rest, line)?;
        Ok(Stmt::GiveBack(expr))
    }

    // ── ask ───────────────────────────────────────────────────────────────

    fn parse_ask(&self, clause: &[Spanned], line: usize) -> Result<Stmt, String> {
        // `ask <prompt> and store in <var>`
        // Find "store in <var>" by scanning the raw token list for the word "store"
        let store_tok_pos = clause.iter().position(|t| matches!(&t.token, Token::Word(w) if w == "store"));
        let store_tok_pos = store_tok_pos.ok_or_else(|| format!("OrimaLang Error: 'ask' missing 'store in' on line {line}"))?;

        let var_name = clause.get(store_tok_pos + 2)
            .and_then(|t| if let Token::Word(w) = &t.token { Some(w.clone()) } else { None })
            .ok_or_else(|| format!("OrimaLang Error: 'ask' missing variable name on line {line}"))?;

        // Prompt: tokens after "ask" up to (and excluding) the "and store" or "store"
        let end = if store_tok_pos > 0 {
            if let Token::Word(w) = &clause[store_tok_pos - 1].token {
                if w == "and" { store_tok_pos - 1 } else { store_tok_pos }
            } else {
                store_tok_pos
            }
        } else {
            store_tok_pos
        };

        let prompt_parts: Vec<String> = clause[1..end].iter().filter_map(|t| match &t.token {
            Token::Word(w) => Some(w.clone()),
            Token::StringLit(s) => Some(s.clone()),
            _ => None,
        }).collect();

        Ok(Stmt::Ask(prompt_parts, var_name))
    }

    // ── Expression parsers ────────────────────────────────────────────────

    /// Parse a general expression (handles conditions, logical ops, math, values).
    fn parse_expr(&self, tokens: &[Spanned], line: usize) -> Result<Expr, String> {
        if tokens.is_empty() {
            return Err(format!("OrimaLang Error: expected expression on line {line}"));
        }
        // Check for logical operators: "and also", "or"
        // Scan for top-level "or" first (lower precedence)
        if let Some(pos) = self.find_logic_op(tokens, "or") {
            let left = self.parse_expr(&tokens[..pos], line)?;
            let right = self.parse_expr(&tokens[pos + 1..], line)?;
            return Ok(Expr::LogicalOp(Box::new(left), LogicOp::Or, Box::new(right)));
        }
        // "and also"
        if let Some(pos) = self.find_and_also(tokens) {
            let left = self.parse_expr(&tokens[..pos], line)?;
            let right = self.parse_expr(&tokens[pos + 2..], line)?;
            return Ok(Expr::LogicalOp(Box::new(left), LogicOp::And, Box::new(right)));
        }
        // "it is not the case that"
        if Self::starts_with_words(tokens, &["it", "is", "not", "the", "case", "that"]) {
            let inner = self.parse_expr(&tokens[6..], line)?;
            return Ok(Expr::Not(Box::new(inner)));
        }
        // Try parsing as condition
        self.parse_condition_or_value(tokens, line)
    }

    fn find_logic_op(&self, tokens: &[Spanned], op: &str) -> Option<usize> {
        for (i, t) in tokens.iter().enumerate() {
            if let Token::Word(w) = &t.token {
                if w == op {
                    return Some(i);
                }
            }
        }
        None
    }

    fn find_and_also(&self, tokens: &[Spanned]) -> Option<usize> {
        for i in 0..tokens.len().saturating_sub(1) {
            if let (Token::Word(a), Token::Word(b)) = (&tokens[i].token, &tokens[i + 1].token) {
                if a == "and" && b == "also" {
                    return Some(i);
                }
            }
        }
        None
    }

    fn starts_with_words(tokens: &[Spanned], words: &[&str]) -> bool {
        if tokens.len() < words.len() {
            return false;
        }
        for (i, w) in words.iter().enumerate() {
            if let Token::Word(tw) = &tokens[i].token {
                if tw != w {
                    return false;
                }
            } else {
                return false;
            }
        }
        true
    }

    fn parse_condition_or_value(&self, tokens: &[Spanned], line: usize) -> Result<Expr, String> {
        // Try to detect comparison operator
        // Patterns: "is greater than", "is less than", "is at least", "is at most",
        //           "is not", "is", "contains", "starts with", "ends with"
        if let Some((left_tokens, cmp, right_tokens)) = self.find_comparison(tokens) {
            let left = self.parse_value_expr(left_tokens, line)?;
            let right = self.parse_value_expr(right_tokens, line)?;
            return Ok(Expr::Condition(Box::new(left), cmp, Box::new(right)));
        }
        // Otherwise treat as value/math expression
        self.parse_math_expr(tokens, line)
    }

    /// Try to find a comparison operator in the token stream.
    /// Returns (left_slice, comparison, right_slice) if found.
    fn find_comparison<'a>(&self, tokens: &'a [Spanned]) -> Option<(&'a [Spanned], Comparison, &'a [Spanned])> {
        let n = tokens.len();
        // Scan left to right for comparison patterns
        // Longer patterns first
        for i in 0..n {
            // "is not" (2 words)
            if Self::tok_is_word(&tokens[i], "is") {
                if i + 1 < n && Self::tok_is_word(&tokens[i + 1], "not") {
                    // make sure not "is not the case that"
                    if !(i + 2 < n && Self::tok_is_word(&tokens[i + 2], "the")) {
                        return Some((&tokens[..i], Comparison::IsNot, &tokens[i + 2..]));
                    }
                }
                // "is greater than" (3 words)
                if i + 2 < n && Self::tok_is_word(&tokens[i + 1], "greater") && Self::tok_is_word(&tokens[i + 2], "than") {
                    return Some((&tokens[..i], Comparison::GreaterThan, &tokens[i + 3..]));
                }
                // "is less than" (3 words)
                if i + 2 < n && Self::tok_is_word(&tokens[i + 1], "less") && Self::tok_is_word(&tokens[i + 2], "than") {
                    return Some((&tokens[..i], Comparison::LessThan, &tokens[i + 3..]));
                }
                // "is at least" (3 words)
                if i + 2 < n && Self::tok_is_word(&tokens[i + 1], "at") && Self::tok_is_word(&tokens[i + 2], "least") {
                    return Some((&tokens[..i], Comparison::AtLeast, &tokens[i + 3..]));
                }
                // "is at most" (3 words)
                if i + 2 < n && Self::tok_is_word(&tokens[i + 1], "at") && Self::tok_is_word(&tokens[i + 2], "most") {
                    return Some((&tokens[..i], Comparison::AtMost, &tokens[i + 3..]));
                }
                // plain "is"
                return Some((&tokens[..i], Comparison::Is, &tokens[i + 1..]));
            }
            // "contains"
            if Self::tok_is_word(&tokens[i], "contains") {
                return Some((&tokens[..i], Comparison::Contains, &tokens[i + 1..]));
            }
            // "starts with" (2 words)
            if Self::tok_is_word(&tokens[i], "starts") {
                if i + 1 < n && Self::tok_is_word(&tokens[i + 1], "with") {
                    return Some((&tokens[..i], Comparison::StartsWith, &tokens[i + 2..]));
                }
            }
            // "ends with" (2 words)
            if Self::tok_is_word(&tokens[i], "ends") {
                if i + 1 < n && Self::tok_is_word(&tokens[i + 1], "with") {
                    return Some((&tokens[..i], Comparison::EndsWith, &tokens[i + 2..]));
                }
            }
        }
        None
    }

    fn tok_is_word(t: &Spanned, w: &str) -> bool {
        matches!(&t.token, Token::Word(tw) if tw == w)
    }

    // ── Math expression ───────────────────────────────────────────────────

    fn parse_math_expr(&self, tokens: &[Spanned], line: usize) -> Result<Expr, String> {
        // Look for binary math operators: plus, minus, times, "divided by", "remainder of divided by"
        // "remainder of divided by" is 4 tokens – check longest first
        for i in 0..tokens.len() {
            if Self::tok_is_word(&tokens[i], "remainder") {
                // "remainder of divided by" = 4 tokens from i
                if i + 3 < tokens.len()
                    && Self::tok_is_word(&tokens[i + 1], "of")
                    && Self::tok_is_word(&tokens[i + 2], "divided")
                    && Self::tok_is_word(&tokens[i + 3], "by")
                {
                    let left = self.parse_value_expr(&tokens[..i], line)?;
                    let right = self.parse_value_expr(&tokens[i + 4..], line)?;
                    return Ok(Expr::BinaryOp(Box::new(left), Op::Remainder, Box::new(right)));
                }
            }
            if Self::tok_is_word(&tokens[i], "divided") {
                if i + 1 < tokens.len() && Self::tok_is_word(&tokens[i + 1], "by") {
                    let left = self.parse_value_expr(&tokens[..i], line)?;
                    let right = self.parse_value_expr(&tokens[i + 2..], line)?;
                    return Ok(Expr::BinaryOp(Box::new(left), Op::DividedBy, Box::new(right)));
                }
            }
            if Self::tok_is_word(&tokens[i], "plus") {
                let left = self.parse_value_expr(&tokens[..i], line)?;
                let right = self.parse_value_expr(&tokens[i + 1..], line)?;
                return Ok(Expr::BinaryOp(Box::new(left), Op::Plus, Box::new(right)));
            }
            if Self::tok_is_word(&tokens[i], "minus") {
                let left = self.parse_value_expr(&tokens[..i], line)?;
                let right = self.parse_value_expr(&tokens[i + 1..], line)?;
                return Ok(Expr::BinaryOp(Box::new(left), Op::Minus, Box::new(right)));
            }
            if Self::tok_is_word(&tokens[i], "times") {
                let left = self.parse_value_expr(&tokens[..i], line)?;
                let right = self.parse_value_expr(&tokens[i + 1..], line)?;
                return Ok(Expr::BinaryOp(Box::new(left), Op::Times, Box::new(right)));
            }
        }
        // No operator found – single value
        self.parse_value_expr(tokens, line)
    }

    // ── Value expression (atom) ───────────────────────────────────────────

    fn parse_value_expr(&self, tokens: &[Spanned], line: usize) -> Result<Expr, String> {
        if tokens.is_empty() {
            return Err(format!("OrimaLang Error: expected value expression on line {line}"));
        }

        // StringLit: handle early before other checks
        if tokens.len() == 1 {
            if let Token::StringLit(s) = &tokens[0].token {
                return Ok(Expr::Literal(Value::Str(s.clone())));
            }
        }
        // Multi-token starting with StringLit — treat first as the string
        if let Token::StringLit(s) = &tokens[0].token {
            if tokens.len() == 1 {
                return Ok(Expr::Literal(Value::Str(s.clone())));
            }
        }

        // `item N from <list>` (3+ tokens)
        if Self::tok_is_word(&tokens[0], "item") && tokens.len() >= 3 {
            if let Some(from_pos) = tokens.iter().position(|t| Self::tok_is_word(t, "from")) {
                let idx_tokens = &tokens[1..from_pos];
                let list_name = match &tokens[from_pos + 1].token {
                    Token::Word(w) => w.clone(),
                    _ => return Err(format!("OrimaLang Error: expected list name after 'from' on line {line}")),
                };
                let idx_expr = self.parse_expr(idx_tokens, line)?;
                return Ok(Expr::ItemFrom(Box::new(idx_expr), list_name));
            }
        }

        // `the size of <list>`
        if Self::starts_with_words(tokens, &["the", "size", "of"]) {
            let name = match tokens.get(3) {
                Some(t) => match &t.token {
                    Token::Word(w) => w.clone(),
                    _ => return Err(format!("OrimaLang Error: expected list name in 'size of' on line {line}")),
                },
                None => return Err(format!("OrimaLang Error: expected list name in 'size of' on line {line}")),
            };
            return Ok(Expr::SizeOf(name));
        }

        // `the length of <var>`
        if Self::starts_with_words(tokens, &["the", "length", "of"]) {
            let name = match tokens.get(3) {
                Some(t) => match &t.token {
                    Token::Word(w) => w.clone(),
                    _ => return Err(format!("OrimaLang Error: expected variable name in 'length of' on line {line}")),
                },
                None => return Err(format!("OrimaLang Error: expected variable name in 'length of' on line {line}")),
            };
            return Ok(Expr::LengthOf(name));
        }

        // `<x> joined with <y>`
        if let Some(jw_pos) = tokens.iter().position(|t| Self::tok_is_word(t, "joined")) {
            if jw_pos + 1 < tokens.len() && Self::tok_is_word(&tokens[jw_pos + 1], "with") {
                let left = self.parse_value_expr(&tokens[..jw_pos], line)?;
                let right = self.parse_value_expr(&tokens[jw_pos + 2..], line)?;
                return Ok(Expr::JoinedWith(Box::new(left), Box::new(right)));
            }
        }

        // `<var> in uppercase` / `<var> in lowercase`
        if tokens.len() >= 3 {
            let last = tokens.last().unwrap();
            let second_last = &tokens[tokens.len() - 2];
            if Self::tok_is_word(second_last, "in") {
                if Self::tok_is_word(last, "uppercase") {
                    let inner = self.parse_value_expr(&tokens[..tokens.len() - 2], line)?;
                    return Ok(Expr::InUppercase(Box::new(inner)));
                }
                if Self::tok_is_word(last, "lowercase") {
                    let inner = self.parse_value_expr(&tokens[..tokens.len() - 2], line)?;
                    return Ok(Expr::InLowercase(Box::new(inner)));
                }
            }
        }

        // `the value of <var>`
        if Self::starts_with_words(tokens, &["the", "value", "of"]) {
            let name = match tokens.get(3) {
                Some(t) => match &t.token {
                    Token::Word(w) => w.clone(),
                    _ => return Err(format!("OrimaLang Error: expected variable name in 'value of' on line {line}")),
                },
                None => return Err(format!("OrimaLang Error: expected variable name in 'value of' on line {line}")),
            };
            return Ok(Expr::Variable(name));
        }

        // `<field> of <obj>` — e.g. color of self (no "the" required)
        if let Some(of_pos) = tokens.iter().position(|t| Self::tok_is_word(t, "of")) {
            if of_pos >= 1 && of_pos + 1 < tokens.len() {
                // field start: skip "the" if present
                let field_start = if Self::tok_is_word(&tokens[0], "the") { 1 } else { 0 };
                if of_pos > field_start {
                    let field_name = tokens[field_start..of_pos].iter()
                        .filter_map(|t| if let Token::Word(w) = &t.token { Some(w.clone()) } else { None })
                        .collect::<Vec<_>>()
                        .join(" ");
                    if let Some(obj_tok) = tokens.get(of_pos + 1) {
                        if let Token::Word(obj_name) = &obj_tok.token {
                            if !field_name.is_empty() && tokens.len() == of_pos + 2 {
                                return Ok(Expr::GetField(obj_name.clone(), field_name));
                            }
                        }
                    }
                }
            }
        }

        // `the <field> of <objvar>` — after existing specific "the X of" checks
        if Self::tok_is_word(&tokens[0], "the") && tokens.len() >= 3 {
            if let Some(of_pos) = tokens.iter().position(|t| Self::tok_is_word(t, "of")) {
                if of_pos >= 2 {
                    let field_name = tokens[1..of_pos].iter()
                        .filter_map(|t| if let Token::Word(w) = &t.token { Some(w.clone()) } else { None })
                        .collect::<Vec<_>>()
                        .join(" ");
                    if !field_name.is_empty() {
                        if let Some(obj_tok) = tokens.get(of_pos + 1) {
                            if let Token::Word(obj_name) = &obj_tok.token {
                                return Ok(Expr::GetField(obj_name.clone(), field_name));
                            }
                        }
                    }
                }
            }
        }

        // single token
        if tokens.len() == 1 {
            return self.parse_single_token(&tokens[0], line);
        }

        // Multi-word: error (use quotes for string literals)
        Err(format!("OrimaLang Error: unexpected multi-word expression on line {line}; did you forget quotes?"))
    }

    fn parse_single_token(&self, t: &Spanned, line: usize) -> Result<Expr, String> {
        match &t.token {
            Token::Number(n) => Ok(Expr::Literal(Value::Num(*n))),
            Token::StringLit(s) => Ok(Expr::Literal(Value::Str(s.clone()))),
            Token::Word(w) => {
                match w.as_str() {
                    "true" => Ok(Expr::Literal(Value::Bool(true))),
                    "false" => Ok(Expr::Literal(Value::Bool(false))),
                    "nil" | "nothing" => Ok(Expr::Literal(Value::Nil)),
                    _ => Ok(Expr::Variable(w.clone())),
                }
            }
            _ => Err(format!("OrimaLang Error: unexpected token in expression on line {line}")),
        }
    }

    /// Parse a condition expression (for if/while).
    pub fn parse_condition_expr(&self, tokens: &[Spanned], line: usize) -> Result<Expr, String> {
        self.parse_expr(tokens, line)
    }
}

fn format_number(n: f64) -> String {
    if n == n.floor() && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        format!("{}", n)
    }
}

// ─── Public entry point ────────────────────────────────────────────────────────

pub fn parse(tokens: Vec<Spanned>) -> Result<Vec<Stmt>, String> {
    let mut parser = Parser::new(tokens);
    parser.parse_program()
}
