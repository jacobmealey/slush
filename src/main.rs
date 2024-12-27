use std::fs::File;
use std::io::{self, BufRead, Write};
use std::path::Path;
use std::process::{Command, Stdio};

use crate::parser::tokenizer::tokenizer;
use crate::tokenizer::ShTokenType;
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
        let cmd =tokenizer::tokens(&line);
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
