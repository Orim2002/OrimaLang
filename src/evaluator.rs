#![allow(dead_code)]
use std::collections::HashMap;
use std::io::{self, BufRead, Write};

use crate::parser::{Comparison, Expr, LogicOp, Op, Stmt, Value};

// ─── Signals ──────────────────────────────────────────────────────────────────

/// Signals returned from statement execution to control flow.
#[derive(Debug)]
pub enum Signal {
    None,
    Return(Value),
    Break,
    Continue,
}

// ─── Environment ─────────────────────────────────────────────────────────────

/// A lexically-scoped environment.  The last frame is the innermost scope.
pub struct Env {
    frames: Vec<HashMap<String, Value>>,
    /// Captured function definitions: name -> (params, body)
    pub functions: HashMap<String, (Vec<String>, Vec<Stmt>)>,
    /// Class definitions: name -> (fields, methods)
    pub classes: HashMap<String, (Vec<String>, Vec<(String, Vec<String>, Vec<Stmt>)>)>,
    /// Output buffer – used by the WASM / test path instead of stdout
    pub output_buffer: Option<Vec<String>>,
}

impl Env {
    pub fn new() -> Self {
        Env {
            frames: vec![HashMap::new()],
            functions: HashMap::new(),
            classes: HashMap::new(),
            output_buffer: None,
        }
    }

    pub fn push_frame(&mut self) {
        self.frames.push(HashMap::new());
    }

    pub fn pop_frame(&mut self) {
        if self.frames.len() > 1 {
            self.frames.pop();
        }
    }

    /// Look up a variable searching from innermost to outermost scope.
    pub fn get(&self, name: &str) -> Option<&Value> {
        for frame in self.frames.iter().rev() {
            if let Some(v) = frame.get(name) {
                return Some(v);
            }
        }
        None
    }

    /// Set a variable in the innermost scope that already has it,
    /// or create it in the current (innermost) scope.
    pub fn set(&mut self, name: &str, value: Value) {
        // If the variable already exists somewhere, update there
        for frame in self.frames.iter_mut().rev() {
            if frame.contains_key(name) {
                frame.insert(name.to_string(), value);
                return;
            }
        }
        // Otherwise create in innermost scope
        let last = self.frames.last_mut().unwrap();
        last.insert(name.to_string(), value);
    }

    /// Always create / update in the current (innermost) scope.
    pub fn set_local(&mut self, name: &str, value: Value) {
        let last = self.frames.last_mut().unwrap();
        last.insert(name.to_string(), value);
    }

    fn emit_line(&mut self, line: &str) {
        if let Some(buf) = &mut self.output_buffer {
            buf.push(line.to_string());
        } else {
            println!("{}", line);
        }
    }
}

// ─── Evaluator ───────────────────────────────────────────────────────────────

pub fn eval_program(stmts: &[Stmt], env: &mut Env) -> Result<(), String> {
    for stmt in stmts {
        let sig = eval_stmt(stmt, env)?;
        match sig {
            Signal::None => {}
            Signal::Return(_) | Signal::Break | Signal::Continue => {
                // Top-level – ignore stray signals
            }
        }
    }
    Ok(())
}

pub fn eval_stmt(stmt: &Stmt, env: &mut Env) -> Result<Signal, String> {
    match stmt {
        // ── say ──────────────────────────────────────────────────────────
        Stmt::Say(parts) => {
            let mut pieces: Vec<String> = Vec::new();
            for part in parts {
                let val = eval_expr(part, env)?;
                pieces.push(value_to_string(&val));
            }
            // Join pieces with a single space so that
            // `say your name is and name and nice to meet you` reads naturally.
            let out = pieces.join(" ");
            env.emit_line(&out);
            Ok(Signal::None)
        }

        // ── set ───────────────────────────────────────────────────────────
        Stmt::Set(name, expr) => {
            let val = eval_expr(expr, env)?;
            env.set(name, val);
            Ok(Signal::None)
        }

        // ── increase ──────────────────────────────────────────────────────
        Stmt::Increase(name, expr) => {
            let delta = eval_expr(expr, env)?;
            let current = env.get(name).cloned().unwrap_or(Value::Num(0.0));
            let result = num_op(current, delta, |a, b| a + b, "increase")?;
            env.set(name, result);
            Ok(Signal::None)
        }

        // ── decrease ──────────────────────────────────────────────────────
        Stmt::Decrease(name, expr) => {
            let delta = eval_expr(expr, env)?;
            let current = env.get(name).cloned().unwrap_or(Value::Num(0.0));
            let result = num_op(current, delta, |a, b| a - b, "decrease")?;
            env.set(name, result);
            Ok(Signal::None)
        }

        // ── multiply ──────────────────────────────────────────────────────
        Stmt::Multiply(name, expr) => {
            let factor = eval_expr(expr, env)?;
            let current = env.get(name).cloned().unwrap_or(Value::Num(0.0));
            let result = num_op(current, factor, |a, b| a * b, "multiply")?;
            env.set(name, result);
            Ok(Signal::None)
        }

        // ── divide ────────────────────────────────────────────────────────
        Stmt::Divide(name, expr) => {
            let divisor = eval_expr(expr, env)?;
            let current = env.get(name).cloned().unwrap_or(Value::Num(0.0));
            let result = num_op(current, divisor, |a, b| a / b, "divide")?;
            env.set(name, result);
            Ok(Signal::None)
        }

        // ── if ────────────────────────────────────────────────────────────
        Stmt::If(cond, then_body, else_body) => {
            let cond_val = eval_expr(cond, env)?;
            let branch = if is_truthy(&cond_val) { then_body } else { else_body };
            for s in branch {
                let sig = eval_stmt(s, env)?;
                match sig {
                    Signal::None => {}
                    other => return Ok(other),
                }
            }
            Ok(Signal::None)
        }

        // ── repeat ────────────────────────────────────────────────────────
        Stmt::Repeat(count_expr, body) => {
            let count = eval_expr(count_expr, env)?;
            let n = to_num(&count, "repeat count")? as i64;
            for _ in 0..n {
                for s in body {
                    let sig = eval_stmt(s, env)?;
                    match sig {
                        Signal::None => {}
                        Signal::Continue => break,
                        Signal::Break => return Ok(Signal::None),
                        Signal::Return(v) => return Ok(Signal::Return(v)),
                    }
                }
            }
            Ok(Signal::None)
        }

        // ── while ─────────────────────────────────────────────────────────
        Stmt::While(cond, body) => {
            loop {
                let cv = eval_expr(cond, env)?;
                if !is_truthy(&cv) {
                    break;
                }
                for s in body {
                    let sig = eval_stmt(s, env)?;
                    match sig {
                        Signal::None => {}
                        Signal::Continue => break,
                        Signal::Break => return Ok(Signal::None),
                        Signal::Return(v) => return Ok(Signal::Return(v)),
                    }
                }
            }
            Ok(Signal::None)
        }

        // ── for each ──────────────────────────────────────────────────────
        Stmt::ForEach(item_var, list_var, body) => {
            let list_val = env.get(list_var).cloned()
                .ok_or_else(|| format!("OrimaLang Error: undefined variable '{list_var}'"))?;
            let items = match list_val {
                Value::List(items) => items,
                other => vec![other],
            };
            for item in items {
                env.set(item_var, item);
                for s in body {
                    let sig = eval_stmt(s, env)?;
                    match sig {
                        Signal::None => {}
                        Signal::Continue => break,
                        Signal::Break => return Ok(Signal::None),
                        Signal::Return(v) => return Ok(Signal::Return(v)),
                    }
                }
            }
            Ok(Signal::None)
        }

        // ── stop loop ─────────────────────────────────────────────────────
        Stmt::StopLoop => Ok(Signal::Break),

        // ── skip to next ──────────────────────────────────────────────────
        Stmt::SkipToNext => Ok(Signal::Continue),

        // ── create list ───────────────────────────────────────────────────
        Stmt::CreateList(name) => {
            env.set(name, Value::List(Vec::new()));
            Ok(Signal::None)
        }

        // ── add to list ───────────────────────────────────────────────────
        Stmt::AddToList(val_expr, list_name) => {
            let val = eval_expr(val_expr, env)?;
            let list = env.get(list_name).cloned()
                .ok_or_else(|| format!("OrimaLang Error: undefined list '{list_name}'"))?;
            let mut items = match list {
                Value::List(v) => v,
                _ => return Err(format!("OrimaLang Error: '{list_name}' is not a list")),
            };
            items.push(val);
            env.set(list_name, Value::List(items));
            Ok(Signal::None)
        }

        // ── remove from list (by value) ───────────────────────────────────
        Stmt::RemoveFromList(val_expr, list_name) => {
            let val = eval_expr(val_expr, env)?;
            let list = env.get(list_name).cloned()
                .ok_or_else(|| format!("OrimaLang Error: undefined list '{list_name}'"))?;
            let mut items = match list {
                Value::List(v) => v,
                _ => return Err(format!("OrimaLang Error: '{list_name}' is not a list")),
            };
            let val_str = value_to_string(&val);
            items.retain(|item| value_to_string(item) != val_str);
            env.set(list_name, Value::List(items));
            Ok(Signal::None)
        }

        // ── remove item N from list ───────────────────────────────────────
        Stmt::RemoveItemFromList(idx_expr, list_name) => {
            let idx_val = eval_expr(idx_expr, env)?;
            let idx = to_num(&idx_val, "list index")? as usize;
            let list = env.get(list_name).cloned()
                .ok_or_else(|| format!("OrimaLang Error: undefined list '{list_name}'"))?;
            let mut items = match list {
                Value::List(v) => v,
                _ => return Err(format!("OrimaLang Error: '{list_name}' is not a list")),
            };
            if idx < 1 || idx > items.len() {
                return Err(format!("OrimaLang Error: index {idx} out of range for list '{list_name}'"));
            }
            items.remove(idx - 1);
            env.set(list_name, Value::List(items));
            Ok(Signal::None)
        }

        // ── define ────────────────────────────────────────────────────────
        Stmt::Define(name, params, body) => {
            env.functions.insert(name.clone(), (params.clone(), body.clone()));
            Ok(Signal::None)
        }

        // ── run ───────────────────────────────────────────────────────────
        Stmt::Run(name, arg_exprs, store_var) => {
            let args: Vec<Value> = arg_exprs.iter()
                .map(|e| eval_expr(e, env))
                .collect::<Result<_, _>>()?;

            let result = call_function(name, args, env)?;

            if let Some(var) = store_var {
                env.set(var, result);
            }
            Ok(Signal::None)
        }

        // ── give back ─────────────────────────────────────────────────────
        Stmt::GiveBack(expr) => {
            let val = eval_expr(expr, env)?;
            Ok(Signal::Return(val))
        }

        // ── ask ───────────────────────────────────────────────────────────
        Stmt::Ask(prompt_words, var_name) => {
            let prompt = prompt_words.join(" ");
            if env.output_buffer.is_some() {
                // In buffered (WASM) mode, just store an empty string
                env.set(var_name, Value::Str(String::new()));
            } else {
                print!("{} ", prompt);
                io::stdout().flush().ok();
                let mut input = String::new();
                io::stdin().read_line(&mut input).ok();
                let input = input.trim_end_matches('\n').trim_end_matches('\r').to_string();
                env.set(var_name, Value::Str(input));
            }
            Ok(Signal::None)
        }

        // ── define class ──────────────────────────────────────────────────
        Stmt::DefineClass(name, fields, methods) => {
            env.classes.insert(name.clone(), (fields.clone(), methods.clone()));
            Ok(Signal::None)
        }

        // ── create object ─────────────────────────────────────────────────
        Stmt::CreateObject(class_name, arg_exprs, store_var) => {
            let args: Vec<Value> = arg_exprs.iter()
                .map(|e| eval_expr(e, env))
                .collect::<Result<_, _>>()?;

            let (fields, _methods) = env.classes.get(class_name)
                .cloned()
                .ok_or_else(|| format!("OrimaLang Error: undefined class '{class_name}'"))?;

            let mut fields_map = HashMap::new();
            for (i, field) in fields.iter().enumerate() {
                let val = args.get(i).cloned().unwrap_or(Value::Nil);
                fields_map.insert(field.clone(), val);
            }

            env.set(store_var, Value::Object(class_name.clone(), fields_map));
            Ok(Signal::None)
        }

        // ── set field ─────────────────────────────────────────────────────
        Stmt::SetField(obj_var, field_name, expr) => {
            let value = eval_expr(expr, env)?;
            let obj = env.get(obj_var).cloned()
                .ok_or_else(|| format!("OrimaLang Error: undefined variable '{obj_var}'"))?;
            match obj {
                Value::Object(class_name, mut fields) => {
                    fields.insert(field_name.clone(), value);
                    env.set(obj_var, Value::Object(class_name, fields));
                    Ok(Signal::None)
                }
                _ => Err(format!("OrimaLang Error: '{obj_var}' is not an object")),
            }
        }

        // ── run method ────────────────────────────────────────────────────
        Stmt::RunMethod(method_name, arg_exprs, obj_var, store_var) => {
            let args: Vec<Value> = arg_exprs.iter()
                .map(|e| eval_expr(e, env))
                .collect::<Result<_, _>>()?;

            let obj = env.get(obj_var).cloned()
                .ok_or_else(|| format!("OrimaLang Error: undefined variable '{obj_var}'"))?;

            let (class_name, obj_fields) = match obj {
                Value::Object(cn, fields) => (cn, fields),
                _ => return Err(format!("OrimaLang Error: '{obj_var}' is not an object")),
            };

            let (_fields_def, methods) = env.classes.get(&class_name)
                .cloned()
                .ok_or_else(|| format!("OrimaLang Error: undefined class '{class_name}'"))?;

            let (params, body) = methods.iter()
                .find(|(name, _, _)| name == method_name)
                .map(|(_, p, b)| (p.clone(), b.clone()))
                .ok_or_else(|| format!("OrimaLang Error: undefined method '{method_name}' on class '{class_name}'"))?;

            env.push_frame();

            // Bind `self` to the full object so methods can use self.field
            env.set_local("self", Value::Object(class_name.clone(), obj_fields.clone()));

            // Also bind each field as a local variable (backwards compat)
            for (fname, fval) in &obj_fields {
                env.set_local(fname, fval.clone());
            }

            // Set each parameter from args
            for (i, param) in params.iter().enumerate() {
                let val = args.get(i).cloned().unwrap_or(Value::Nil);
                env.set_local(param, val);
            }

            let mut result = Value::Nil;
            for s in &body {
                let sig = eval_stmt(s, env)?;
                match sig {
                    Signal::None => {}
                    Signal::Return(v) => {
                        result = v;
                        break;
                    }
                    Signal::Break | Signal::Continue => break,
                }
            }

            // Sync self back to the original object variable so mutations persist
            if let Some(Value::Object(_, updated_fields)) = env.get("self").cloned() {
                env.pop_frame();
                env.set(obj_var, Value::Object(class_name, updated_fields));
            } else {
                env.pop_frame();
            }

            if let Some(var) = store_var {
                env.set(var, result);
            }
            Ok(Signal::None)
        }
    }
}

// ─── Function call ─────────────────────────────────────────────────────────

fn call_function(name: &str, args: Vec<Value>, env: &mut Env) -> Result<Value, String> {
    let (params, body) = env.functions.get(name)
        .cloned()
        .ok_or_else(|| format!("OrimaLang Error: undefined function '{name}'"))?;

    env.push_frame();
    for (i, param) in params.iter().enumerate() {
        let val = args.get(i).cloned().unwrap_or(Value::Nil);
        env.set_local(param, val);
    }

    let mut result = Value::Nil;
    for stmt in &body {
        let sig = eval_stmt(stmt, env)?;
        match sig {
            Signal::None => {}
            Signal::Return(v) => {
                result = v;
                break;
            }
            Signal::Break | Signal::Continue => break,
        }
    }

    env.pop_frame();
    Ok(result)
}

// ─── Expression evaluation ────────────────────────────────────────────────────

pub fn eval_expr(expr: &Expr, env: &mut Env) -> Result<Value, String> {
    match expr {
        Expr::Literal(v) => Ok(v.clone()),

        Expr::Variable(name) => {
            env.get(name).cloned()
                .ok_or_else(|| format!("OrimaLang Error: undefined variable '{}' - did you mean to quote it as \"{}\"?", name, name))
        }

        Expr::BinaryOp(left, op, right) => {
            let lv = eval_expr(left, env)?;
            let rv = eval_expr(right, env)?;
            match op {
                Op::Plus => {
                    match (&lv, &rv) {
                        (Value::Num(a), Value::Num(b)) => Ok(Value::Num(a + b)),
                        _ => Ok(Value::Str(value_to_string(&lv) + &value_to_string(&rv))),
                    }
                }
                Op::Minus => {
                    let a = to_num(&lv, "subtraction")?;
                    let b = to_num(&rv, "subtraction")?;
                    Ok(Value::Num(a - b))
                }
                Op::Times => {
                    let a = to_num(&lv, "multiplication")?;
                    let b = to_num(&rv, "multiplication")?;
                    Ok(Value::Num(a * b))
                }
                Op::DividedBy => {
                    let a = to_num(&lv, "division")?;
                    let b = to_num(&rv, "division")?;
                    if b == 0.0 {
                        return Err("OrimaLang Error: division by zero".to_string());
                    }
                    Ok(Value::Num(a / b))
                }
                Op::Remainder => {
                    let a = to_num(&lv, "remainder")?;
                    let b = to_num(&rv, "remainder")?;
                    if b == 0.0 {
                        return Err("OrimaLang Error: division by zero in remainder".to_string());
                    }
                    Ok(Value::Num(a % b))
                }
            }
        }

        Expr::Condition(left, cmp, right) => {
            let lv = eval_expr(left, env)?;
            let rv = eval_expr(right, env)?;
            let result = eval_comparison(&lv, cmp, &rv)?;
            Ok(Value::Bool(result))
        }

        Expr::LogicalOp(left, op, right) => {
            let lv = eval_expr(left, env)?;
            match op {
                LogicOp::And => {
                    if !is_truthy(&lv) {
                        return Ok(Value::Bool(false));
                    }
                    let rv = eval_expr(right, env)?;
                    Ok(Value::Bool(is_truthy(&rv)))
                }
                LogicOp::Or => {
                    if is_truthy(&lv) {
                        return Ok(Value::Bool(true));
                    }
                    let rv = eval_expr(right, env)?;
                    Ok(Value::Bool(is_truthy(&rv)))
                }
            }
        }

        Expr::Not(inner) => {
            let iv = eval_expr(inner, env)?;
            Ok(Value::Bool(!is_truthy(&iv)))
        }

        Expr::ItemFrom(idx_expr, list_name) => {
            let idx_val = eval_expr(idx_expr, env)?;
            let idx = to_num(&idx_val, "list index")? as usize;
            let list = env.get(list_name).cloned()
                .ok_or_else(|| format!("OrimaLang Error: undefined list '{list_name}'"))?;
            match list {
                Value::List(items) => {
                    if idx < 1 || idx > items.len() {
                        return Err(format!("OrimaLang Error: index {idx} out of range for list '{list_name}'"));
                    }
                    Ok(items[idx - 1].clone())
                }
                _ => Err(format!("OrimaLang Error: '{list_name}' is not a list")),
            }
        }

        Expr::SizeOf(name) => {
            let val = env.get(name).cloned()
                .ok_or_else(|| format!("OrimaLang Error: undefined variable '{name}'"))?;
            match val {
                Value::List(items) => Ok(Value::Num(items.len() as f64)),
                _ => Err(format!("OrimaLang Error: '{name}' is not a list")),
            }
        }

        Expr::LengthOf(name) => {
            let val = env.get(name).cloned()
                .ok_or_else(|| format!("OrimaLang Error: undefined variable '{name}'"))?;
            let s = value_to_string(&val);
            Ok(Value::Num(s.chars().count() as f64))
        }

        Expr::JoinedWith(left, right) => {
            let lv = eval_expr(left, env)?;
            let rv = eval_expr(right, env)?;
            Ok(Value::Str(value_to_string(&lv) + &value_to_string(&rv)))
        }

        Expr::InUppercase(inner) => {
            let iv = eval_expr(inner, env)?;
            Ok(Value::Str(value_to_string(&iv).to_uppercase()))
        }

        Expr::InLowercase(inner) => {
            let iv = eval_expr(inner, env)?;
            Ok(Value::Str(value_to_string(&iv).to_lowercase()))
        }

        Expr::Concat(parts) => {
            let mut out = String::new();
            for part in parts {
                let v = eval_expr(part, env)?;
                out.push_str(&value_to_string(&v));
            }
            Ok(Value::Str(out))
        }

        Expr::GetField(obj_var, field_name) => {
            let obj = env.get(obj_var).cloned()
                .ok_or_else(|| format!("OrimaLang Error: undefined variable '{obj_var}'"))?;
            match obj {
                Value::Object(_, fields) => {
                    Ok(fields.get(field_name).cloned().unwrap_or(Value::Nil))
                }
                _ => Err(format!("OrimaLang Error: '{obj_var}' is not an object")),
            }
        }
    }
}

// ─── Comparison ───────────────────────────────────────────────────────────────

fn eval_comparison(left: &Value, cmp: &Comparison, right: &Value) -> Result<bool, String> {
    match cmp {
        Comparison::Is => Ok(values_equal(left, right)),
        Comparison::IsNot => Ok(!values_equal(left, right)),
        Comparison::GreaterThan => {
            let a = to_num(left, "comparison")?;
            let b = to_num(right, "comparison")?;
            Ok(a > b)
        }
        Comparison::LessThan => {
            let a = to_num(left, "comparison")?;
            let b = to_num(right, "comparison")?;
            Ok(a < b)
        }
        Comparison::AtLeast => {
            let a = to_num(left, "comparison")?;
            let b = to_num(right, "comparison")?;
            Ok(a >= b)
        }
        Comparison::AtMost => {
            let a = to_num(left, "comparison")?;
            let b = to_num(right, "comparison")?;
            Ok(a <= b)
        }
        Comparison::Contains => {
            let haystack = value_to_string(left).to_lowercase();
            let needle = value_to_string(right).to_lowercase();
            Ok(haystack.contains(&needle))
        }
        Comparison::StartsWith => {
            let s = value_to_string(left).to_lowercase();
            let prefix = value_to_string(right).to_lowercase();
            Ok(s.starts_with(&prefix))
        }
        Comparison::EndsWith => {
            let s = value_to_string(left).to_lowercase();
            let suffix = value_to_string(right).to_lowercase();
            Ok(s.ends_with(&suffix))
        }
    }
}

fn values_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Num(x), Value::Num(y)) => x == y,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Nil, Value::Nil) => true,
        (Value::Str(x), Value::Str(y)) => x.to_lowercase() == y.to_lowercase(),
        (Value::Object(ca, fa), Value::Object(cb, fb)) => ca == cb && fa == fb,
        // Cross-type: convert both to string and compare case-insensitively
        _ => value_to_string(a).to_lowercase() == value_to_string(b).to_lowercase(),
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

pub fn value_to_string(v: &Value) -> String {
    match v {
        Value::Str(s) => s.clone(),
        Value::Num(n) => format_number(*n),
        Value::Bool(b) => if *b { "true".to_string() } else { "false".to_string() },
        Value::List(items) => {
            let parts: Vec<String> = items.iter().map(value_to_string).collect();
            format!("[{}]", parts.join(", "))
        }
        Value::Nil => "nothing".to_string(),
        Value::Object(class, fields) => {
            let mut pairs: Vec<String> = fields.iter()
                .map(|(k, v)| format!("{}: {}", k, value_to_string(v)))
                .collect();
            pairs.sort(); // deterministic output
            format!("{}({})", class, pairs.join(", "))
        }
    }
}

fn format_number(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        // Remove trailing zeros
        let s = format!("{:.10}", n);
        let s = s.trim_end_matches('0');
        let s = s.trim_end_matches('.');
        s.to_string()
    }
}

fn is_truthy(v: &Value) -> bool {
    match v {
        Value::Bool(b) => *b,
        Value::Nil => false,
        Value::Num(n) => *n != 0.0,
        Value::Str(s) => !s.is_empty(),
        Value::List(l) => !l.is_empty(),
        Value::Object(_, _) => true,
    }
}

fn to_num(v: &Value, ctx: &str) -> Result<f64, String> {
    match v {
        Value::Num(n) => Ok(*n),
        Value::Str(s) => s.parse::<f64>()
            .map_err(|_| format!("OrimaLang Error: expected number in {ctx}, got '{s}'")),
        Value::Bool(b) => Ok(if *b { 1.0 } else { 0.0 }),
        _ => Err(format!("OrimaLang Error: expected number in {ctx}")),
    }
}

fn num_op(a: Value, b: Value, op: fn(f64, f64) -> f64, ctx: &str) -> Result<Value, String> {
    let an = to_num(&a, ctx)?;
    let bn = to_num(&b, ctx)?;
    Ok(Value::Num(op(an, bn)))
}
