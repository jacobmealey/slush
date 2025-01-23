use std::io::{self, BufRead, Write};
use std::env;
use crate::parser::tokenizer;
mod expr;
mod parser;

fn repl() {
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let mut code = 0;
    if let Some(arg) = env::args().nth(1) {
        let code_str = std::fs::read_to_string(arg).expect("Error reading file");
        let mut parser = parser::Parser::new(&code_str);
        parser.parse();
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
        loop {
            print!("[{}] $ ", code);
            stdout.flush().expect("Error flushing to stdout");
            let line = stdin.lock().lines().next().unwrap().unwrap();
            let mut parser = parser::Parser::new(&line);
            parser.parse();
            if !parser.err.is_empty() {
                println!("{}", parser.err);
                continue;
            } else {
                for mut expr in parser.exprs {
                    code = expr.eval();
                }
            }
        }
    }
}

fn main() {
    repl();
}
