use crate::parser::tokenizer;
use std::env;
use std::io::{self, BufRead, Write};
mod expr;
mod parser;

extern "C" {
    fn kill(pid: u32, sig: u32) -> u32;
}

fn repl() {
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let mut code = 0;
    let state = expr::State::new();
    if let Some(arg) = env::args().nth(1) {
        let code_str = std::fs::read_to_string(arg).expect("Error reading file");
        let mut parser = parser::Parser::new(state);
        parser.parse(&code_str);
        if !parser.err.is_empty() {
            println!("{}", parser.err);
            return;
        } else {
            for mut expr in parser.exprs {
                code = expr.eval();
            }
        }
        std::process::exit(code);
    } else {
        println!("Hello, Slush!");
        let s = state.clone();
        ctrlc::set_handler(move || {
            for child in &mut s.lock().expect("Could not unlock jobs").jobs {
                unsafe {
                    kill(*child, 2);
                }
            }
            s.lock().expect("Could not unlock jobs").jobs.clear();
        })
        .expect("Error ignoring control C");
        loop {
            print!("[{}] $ ", code);
            stdout.flush().expect("Error flushing to stdout");
            let line = stdin.lock().lines().next().unwrap().unwrap();
            let mut parser = parser::Parser::new(state.clone());
            parser.parse(&line);
            if !parser.err.is_empty() {
                println!("{}", parser.err);
                continue;
            } else {
                for expr in &mut parser.exprs {
                    code = expr.eval();
                }
            }
        }
    }
}

fn main() {
    repl();
}
