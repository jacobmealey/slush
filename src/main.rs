use std::io::{self, BufRead, Write};

use crate::parser::tokenizer;
use crate::expr::Expr;
use crate::expr::Evalable;
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
        for expr in parser.exprs {
            code = match expr {
                Expr::PipeLineExpr(mut e) => e.eval(),
                Expr::AssignmentExpr(mut ass) => ass.eval()
            }
        }
    }
}

fn main() {
    println!("Hello, Slush!");
    repl();
}
