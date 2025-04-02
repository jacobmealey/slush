use crate::parser::tokenizer;
use std::env;
use std::io::{self, BufRead, Write};
mod expr;
mod parser;

fn repl() {
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let state = expr::State::new();
    if let Some(arg) = env::args().nth(1) {
        let code_str = std::fs::read_to_string(arg).expect("Error reading file");
        let s = state.clone();
        let mut parser = parser::Parser::new(s.clone());
        parser.parse(&code_str);
        if !parser.err.is_empty() {
            println!("{}", parser.err);
            return;
        } else {
            for mut expr in parser.exprs {
                s.lock().unwrap().prev_status = expr.eval();
            }
        }
        std::process::exit(state.clone().lock().unwrap().prev_status);
    } else {
        println!("Hello, Slush!");
        let s = state.clone();
        let handler_state = state.clone();
        ctrlc::set_handler(move || {
            for child in &mut handler_state.lock().expect("Could not unlock jobs").fg_jobs {
                child.kill().unwrap();
            }
            println!();
            handler_state
                .lock()
                .expect("Could not unlock jobs")
                .fg_jobs
                .clear();
        })
        .expect("Error ignoring control C");
        loop {
            print!("[{}] $ ", state.lock().unwrap().prev_status);
            stdout.flush().expect("Error flushing to stdout");
            let line = match stdin.lock().lines().next() {
                Some(Ok(line)) => line,
                _ => break,
            };
            let mut parser = parser::Parser::new(state.clone());
            parser.parse(&line);
            if !parser.err.is_empty() {
                println!("{}", parser.err);
                continue;
            } else {
                for expr in &mut parser.exprs {
                    s.lock().unwrap().prev_status = expr.eval();
                }
            }

            s.lock().expect("Could not unlock jobs").fg_jobs.clear();
        }
    }
}

fn main() {
    repl();
}
