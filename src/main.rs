#![allow(dead_code)]
use std::env;
use std::fs;
use std::io::{self, BufRead, Write};

use orimalang::run_program_interactive;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage:");
        eprintln!("  orima <file.ori>    Run an OrimaLang source file");
        eprintln!("  orima repl          Start the interactive REPL");
        std::process::exit(1);
    }

    match args[1].as_str() {
        "repl" => {
            run_repl();
        }

        path => {
            let source = fs::read_to_string(path).unwrap_or_else(|e| {
                eprintln!("Error reading '{}': {}", path, e);
                std::process::exit(1);
            });
            if let Err(e) = run_program_interactive(&source) {
                eprintln!("{}", e);
                std::process::exit(1);
            }
        }
    }
}

/// Interactive REPL.
///
/// Input is buffered until a line ending with `.` is encountered (a complete statement).
/// Multi-line statements are supported: keep typing until the statement ends with `.`.
fn run_repl() {
    println!("OrimaLang REPL  (type 'quit.' to exit)");
    println!("Statements end with a period. Multi-line input is supported.");
    println!();

    // We hold on to the environment across REPL entries for a stateful session.
    use orimalang::{ReplState};
    let mut state = ReplState::new();

    let stdin = io::stdin();
    let mut buffer = String::new();

    loop {
        if buffer.trim().is_empty() {
            print!(">>> ");
        } else {
            print!("... ");
        }
        io::stdout().flush().ok();

        let mut line = String::new();
        match stdin.lock().read_line(&mut line) {
            Ok(0) => break, // EOF
            Ok(_) => {}
            Err(e) => {
                eprintln!("Read error: {}", e);
                break;
            }
        }

        let trimmed = line.trim_end();

        if trimmed.to_lowercase() == "quit" || trimmed.to_lowercase() == "quit." {
            println!("Goodbye!");
            break;
        }

        buffer.push_str(trimmed);
        buffer.push(' ');

        // Check if we have a complete statement (buffer contains at least one '.')
        if buffer.contains('.') {
            // Extract all complete statements (up to and including last '.')
            let dot_pos = buffer.rfind('.').unwrap();
            let to_run = buffer[..=dot_pos].trim().to_string();
            buffer = buffer[dot_pos + 1..].trim().to_string();

            if !to_run.is_empty() {
                let output = state.run_snippet(&to_run);
                if !output.is_empty() {
                    print!("{}", output);
                    if !output.ends_with('\n') {
                        println!();
                    }
                }
            }
        }
    }
}
