use std::io::{self, BufRead, Write};

use crate::parser::tokenizer;
use crate::parser::Evalable;
mod parser;
mod runtime;

fn repl() {
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    loop {
        print!("$ ");
        stdout.flush().expect("Error flushing to stdout");
        let line = stdin.lock().lines().next().unwrap().unwrap();
        let mut parser = parser::Parser::new(&line);
        match parser.parse() {
            Ok(mut expr) => {expr.eval();},
            Err(str) => {println!("{}", str);}
        }
    }
}

fn main() {
    println!("Hello, world!");
    repl();
}
