use std::io::{self, BufRead, Write};

use crate::parser::tokenizer;
use crate::expr::Evalable;
mod parser;
mod runtime;
mod expr;

fn repl() {
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let mut code = 0;
    loop {
        print!("[{}] $ ", code);
        stdout.flush().expect("Error flushing to stdout");
        let line = stdin.lock().lines().next().unwrap().unwrap();
        let mut parser = parser::Parser::new(&line);
        match parser.parse() {
            Ok(mut expr) => {code = expr.eval();},
            Err(str) => {println!("{}", str);}
        }
    }
}

fn main() {
    println!("Hello, Slush!");
    repl();
}
