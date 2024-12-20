use std::fs::File;
use std::io::{self, BufRead, Write};
use std::path::Path;
use std::process::{Command, Stdio};

use crate::parser::tokenizer::tokenizer;
mod parser;

fn repl() {
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    loop {
        print!("$ ");
        stdout.flush().expect("Error flushing to stdout");
        let line = stdin.lock().lines().next().unwrap().unwrap();
        let cmd =tokenizer::tokens(line);
        let mut command = Command::new(cmd[0].clone());
        for arg in &cmd[1..] {
            command.arg(arg.clone());
        }
        //let output = command.output().expect("Error processing command {cmd[0]}");
        let output = match command.output() {
            Ok(out) => String::from_utf8(out.stdout).expect("Couldn't parse output"),
            Err(err) => format!("{}\n", err.to_string())
        };
        print!("{}", output);
    }
}

fn main() {
    println!("Hello, world!");
    tokenizer::tokens("poop".to_string());
    repl();
}
