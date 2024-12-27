pub mod tokenizer;
use crate::tokenizer::{
    Token,
    tokens,
    ShTokenType
};
use std::process;
use std::process::Command;
use std::process::Stdio;

pub trait Evalable {
    // evaluate SOME command and provide a return value (0 is success, etc.) 
    fn eval(&mut self) -> i32;
}

// An Expr is a tree of Evalables? 
pub struct Expr{
    command: CommandExpr
}
pub struct CommandExpr {
    command: process::Command
}

pub struct PipeLineExpr {
    pipeline: Vec<Box<dyn Evalable>>
}

impl Evalable for Expr {
    fn eval(&mut self) -> i32 {
        self.command.eval()
    }
}

impl Evalable for CommandExpr {
    fn eval(&mut self) -> i32 {
        let mut code: i32 = 0; 
        let mut child = self.command.spawn().unwrap();
        // {
        //     let mut stdin =  child.stdin.take().unwrap();
        // }
        match child.wait_with_output() {
            Err(e) => { println!("{}", e)},
            Ok(o) => {
                code = o.status.code().expect("Couldn't get exit code");
                print!("{}", String::from_utf8(o.stdout).unwrap());
                
            }
        }
        code
    }
}

impl Evalable for PipeLineExpr {
    fn eval(&mut self) -> i32 {
        let mut lastcode = 0;
        for expr in &mut self.pipeline {
            lastcode = expr.eval()
        }
        lastcode
    }
}

pub struct Parser {
    token: Vec<Token>,
    current: Token,
    prev: Token,
    loc: usize
}

impl Parser {
    pub fn new(line: &String) -> Parser {
        let mut parser = Parser {
            token: tokens(line),
            current: Token { lexeme: "".to_string(), token_type: ShTokenType::EndOfFile},
            prev: Token { lexeme: "".to_string(), token_type: ShTokenType::EndOfFile},
            loc: 0
        };
        parser.current = parser.token[0].clone();
        parser
    }

    pub fn parse(&mut self) -> impl Evalable {
        self.parse_pipeline()
    }

    fn parse_pipeline(&mut self) -> impl Evalable {
        let mut pipeline: Vec<Box<dyn Evalable>> = Vec::new();
        pipeline.push(Box::new(self.parse_command()));
        while self.current.token_type == ShTokenType::Pipe {
             self.next_token();
             pipeline.push(Box::new(self.parse_command()));
        }
        PipeLineExpr {
            pipeline: pipeline
        }
    }

    fn parse_command(&mut self) -> impl Evalable {
        self.skip_whitespace();
        if self.current.token_type != ShTokenType::Name  {
           println!("Syntax error: Expected some command, instead found '{}'.", self.current.lexeme) 
        }
        let mut command = Command::new(self.current.lexeme.clone());
        self.next_token();
        self.skip_whitespace();
        while self.current.token_type == ShTokenType::Name {
            command.arg(self.current.lexeme.clone());
            self.next_token();
            self.skip_whitespace();
        }

        CommandExpr {
            command: command
        }
    }
    
    fn skip_whitespace(&mut self)  {
        while self.current.token_type == ShTokenType::WhiteSpace {
            self.next_token();
        }
    }

    fn next_token(&mut self) {
        // this seems really wasteful but the borrow checker beat me up
        println!("{:?}", self.current);
        self.loc += 1;
        if self.loc >= self.token.len() {
            self.current = Token { lexeme: "".to_string(), token_type: ShTokenType::EndOfFile};
        } else {
            self.current = self.token[self.loc].clone();
            if self.loc > 1 {
                self.prev= self.token[self.loc - 1].clone();
            }
        }
    }
}
