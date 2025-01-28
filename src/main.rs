use crate::parser::tokenizer;
use std::env;
use std::io::{self, BufRead, Write};
mod expr;
mod parser;

fn repl() {
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let mut code = 0;
    let cd: expr::change_dir::ChangeDir = expr::change_dir::ChangeDir::new("/Users/jacobmealey/");
    cd.eval();
    if let Some(arg) = env::args().nth(1) {
        let code_str = std::fs::read_to_string(arg).expect("Error reading file");
        let mut parser = parser::Parser::new();
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
        loop {
            print!("[{}] $ ", code);
            stdout.flush().expect("Error flushing to stdout");
            let line = stdin.lock().lines().next().unwrap().unwrap();
            let mut parser = parser::Parser::new();
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
