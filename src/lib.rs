#![allow(dead_code)]
#![allow(unused_variables)]
#![allow(unused_imports)]

#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;

pub mod lexer;
pub mod parser;
pub mod evaluator;

use evaluator::{Env, eval_program};
use lexer::tokenize;
use parser::parse;

/// Run a complete OrimaLang program, returning all output as a single string.
/// Lines are newline-separated. Errors are included as the return value.
pub fn run_program_internal(source: &str) -> String {
    let tokens = tokenize(source);
    let stmts = match parse(tokens) {
        Ok(s) => s,
        Err(e) => return format!("{}\n", e),
    };
    let mut env = Env::new();
    env.output_buffer = Some(Vec::new());
    match eval_program(&stmts, &mut env) {
        Ok(()) => {}
        Err(e) => {
            if let Some(buf) = &mut env.output_buffer {
                buf.push(e);
            }
        }
    }
    let lines = env.output_buffer.unwrap_or_default();
    let mut out = lines.join("\n");
    if !out.is_empty() {
        out.push('\n');
    }
    out
}

/// Stateful REPL environment that persists across calls.
pub struct ReplState {
    env: Env,
}

impl ReplState {
    pub fn new() -> Self {
        let mut env = Env::new();
        env.output_buffer = None; // Use stdout in REPL mode
        ReplState { env }
    }

    /// Run a snippet (one or more complete statements), returning captured output.
    /// In REPL mode we want to print directly, but for testability we capture.
    pub fn run_snippet(&mut self, source: &str) -> String {
        let tokens = tokenize(source);
        let stmts = match parse(tokens) {
            Ok(s) => s,
            Err(e) => return format!("{}\n", e),
        };
        // Temporarily enable buffering
        self.env.output_buffer = Some(Vec::new());
        match eval_program(&stmts, &mut self.env) {
            Ok(()) => {}
            Err(e) => {
                if let Some(buf) = &mut self.env.output_buffer {
                    buf.push(e);
                }
            }
        }
        let lines = self.env.output_buffer.take().unwrap_or_default();
        let mut out = lines.join("\n");
        if !out.is_empty() {
            out.push('\n');
        }
        out
    }
}

impl Default for ReplState {
    fn default() -> Self {
        Self::new()
    }
}

/// WASM export: run a complete program and return all output as a string.
#[cfg(feature = "wasm")]
#[wasm_bindgen]
pub fn run_program(source: &str) -> String {
    run_program_internal(source)
}
