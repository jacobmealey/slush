pub mod tokenizer;
use crate::tokenizer::{
    Token,
    tokens,
    ShTokenType
};
use std::process;
use std::process::Command;
use std::process::Stdio;
use std::io::Write;

pub trait Evalable {
    // evaluate SOME command and provide a return value (0 is success, etc.) 
    fn eval(&mut self) -> i32;
    fn pipe_in(&mut self, input: String);
    fn pipe_out(&self) -> String;
}


// How do we made these outputs streams? it would be nice to have it feed between
// two child CommandExprs as they are creating them... 
pub struct CommandExpr {
    command: process::Command,
    output: String,
    input: String,
}

pub struct PipeLineExpr {
    pipeline: Vec<Box<dyn Evalable>>
}



impl Evalable for CommandExpr {
    fn eval(&mut self) -> i32 {
        let mut code: i32 = 0; 
        let mut child = match self.command.spawn() {
            Ok(c) => c,
            Err(v) => { println!("{}", v); return 2;} 
        };

        {
            if !self.input.is_empty() {
                let mut stdin = child.stdin.take().unwrap();
                let _ = stdin.write(self.input.as_bytes());
            }
        }
        match child.wait_with_output() {
            Err(e) => { println!("{}", e)},
            Ok(o) => {
                code = o.status.code().expect("Couldn't get exit code");
                self.output = String::from_utf8(o.stdout.clone()).unwrap();
            }
        }
        code
    }

    fn pipe_in(&mut self, input: String) {
        self.input = input;
    }

    fn pipe_out(&self) -> String {
       self.output.clone()
    }

}

impl Evalable for PipeLineExpr {
    fn eval(&mut self) -> i32 {
        let mut lastcode = 0;
        let mut prev_expr: Option<&mut Box<dyn Evalable>> = None; 
        for expr in &mut self.pipeline {
            if let Some(pexpr) = prev_expr {
                expr.pipe_in(pexpr.pipe_out());
            }
            lastcode = expr.eval();
            prev_expr = Some(expr);
        }
        print!("{}", self.pipeline.last().expect("No such lat element").pipe_out());
        lastcode
    }
    fn pipe_in(&mut self, input: String) {
        if self.pipeline.len() > 1 {
            self.pipeline[0].pipe_in(input);
        }
    }

    fn pipe_out(&self) -> String {
        if self.pipeline.len() > 1 {
            return self.pipeline.last().expect("No such last element").pipe_out();
        }
        "".to_string()
    }
}

pub struct Parser {
    token: Vec<Token>,
    current: Token,
    prev: Token,
    loc: usize
}

impl Parser {
    pub fn new(line: &str) -> Parser {
        let mut parser = Parser {
            token: tokens(line),
            current: Token { lexeme: "".to_string(), token_type: ShTokenType::EndOfFile},
            prev: Token { lexeme: "".to_string(), token_type: ShTokenType::EndOfFile},
            loc: 0
        };
        if parser.token.len() > 0 {
            parser.current = parser.token[0].clone();
        }
        parser
    }

    pub fn parse(&mut self) -> Result<impl Evalable, String> {
        self.parse_pipeline()
    }

    fn parse_pipeline(&mut self) -> Result<impl Evalable, String> {
        let mut pipeline: Vec<Box<dyn Evalable>> = Vec::new();
        pipeline.push(Box::new(match self.parse_command() {
            Ok(expr) => expr,
            Err(message) => {return Err(message);} 
        }));
        while self.current.token_type == ShTokenType::Pipe {
             self.next_token();
             pipeline.push(Box::new(match self.parse_command() {
                 Ok(expr) => expr,
                 Err(message) => {return Err(message);} 
             }));
        }
        Ok(PipeLineExpr { pipeline })
    }

    fn parse_command(&mut self) -> Result<impl Evalable, String> {
        self.skip_whitespace();
        if self.current.token_type != ShTokenType::Name  {
           return Err(format!("Syntax error: Expected some command, instead found '{}'.", self.current.lexeme));
        }
        let mut command = Command::new(self.current.lexeme.clone());
        self.next_token();
        self.skip_whitespace();
        while self.current.token_type == ShTokenType::Name {
            command.arg(self.current.lexeme.clone());
            self.next_token();
            self.skip_whitespace();
        }
        command.stdin(Stdio::piped()).stdout(Stdio::piped());
        Ok(CommandExpr { command, output: "".to_string(), input: "".to_string()})
    }
    
    fn skip_whitespace(&mut self)  {
        while self.current.token_type == ShTokenType::WhiteSpace {
            self.next_token();
        }
    }

    fn next_token(&mut self) {
        // this seems really wasteful but the borrow checker beat me up -- how do we change current 
        // and prev to be references?
        //println!("{:?}", self.current);
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
