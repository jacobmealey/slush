use std::io::{self, BufRead, Write};

use crate::parser::tokenizer;
mod parser;
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
        parser.parse();
        for mut expr in parser.exprs {

            code = expr.eval();
        }
    }
}

fn main() {
    println!("Hello, Slush!");
    repl();
}
