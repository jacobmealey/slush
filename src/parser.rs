pub mod tokenizer;
use crate::tokenizer::{
    Token,
    tokens,
    ShTokenType
};
use std::process::Command;
use crate::expr::Evalable;
use crate::expr::CommandExpr;
use crate::expr::PipeLineExpr;
use crate::expr::AssignmentExpr;


pub struct Parser {
    token: Vec<Token>,
    pub exprs: Vec<Box<dyn Evalable>>,
    current: Token,
    prev: Token,
    loc: usize
}

impl Parser {
    pub fn new(line: &str) -> Parser {
        let mut parser = Parser {
            token: tokens(line),
            exprs: Vec::new(),
            current: Token { lexeme: "".to_string(), token_type: ShTokenType::EndOfFile},
            prev: Token { lexeme: "".to_string(), token_type: ShTokenType::EndOfFile},
            loc: 0
        };
        if !parser.token.is_empty() {
            parser.current = parser.token[0].clone();
        }
        parser
    }

    pub fn parse(&mut self) {
        self.parse_pipeline();
    }

    fn parse_pipeline(&mut self) {
        let mut pipeline: Vec<CommandExpr> = Vec::new();
        pipeline.push(match self.parse_command() {
            Ok(expr) => expr,
            Err(message) => {println!("{}", message); return;} 
        });
        while self.current.token_type == ShTokenType::Pipe {
             self.next_token();
             pipeline.push(match self.parse_command() {
                 Ok(expr) => expr,
                 Err(message) => {println!("{}", message); return;} 
             });
        }
        self.exprs.push(Box::new(PipeLineExpr { pipeline }));
    }

    fn parse_command(&mut self) -> Result<CommandExpr, String> {
        self.skip_whitespace();
        self.parse_assignment();
        if self.current.token_type != ShTokenType::Name  {
           return Err(
               format!(
                   "Syntax error: Expected some command, instead found '{:?}'.", 
                   self.current
                   )
               );
        }
        let mut command = Command::new(self.current.lexeme.clone());
        self.next_token();
        self.skip_whitespace();
        if self.current.token_type == ShTokenType::SingleQuote {
            command.arg(self.parse_quoted_string());
        }
        while self.current.token_type == ShTokenType::Name {
            command.arg(self.current.lexeme.clone());
            self.next_token();
            self.skip_whitespace();
            if self.current.token_type == ShTokenType::SingleQuote {
                command.arg(self.parse_quoted_string());
            }
        }
        Ok(CommandExpr { command })
    }

    fn parse_assignment(&mut self) {
        let current_location = self.loc;
        let mut key: String = String::from("");
        let mut val: String = String::from("");
        if self.current.token_type == ShTokenType::Name {
            key = self.current.lexeme.clone();
            self.next_token();
            if self.current.token_type == ShTokenType::Equal {
                self.next_token();
                if self.current.token_type == ShTokenType::SingleQuote {
                   val = self.parse_quoted_string();
                } else if self.current.token_type == ShTokenType::Name {
                    val =  self.current.lexeme.clone();
                    self.next_token();
                }
            }
        }
        if val.len() > 0 {
            self.exprs.push(Box::new(AssignmentExpr{ key, val }));
        } else {
            self.loc = current_location;
            self.current =  self.token[self.loc].clone();
        }
        self.skip_whitespace();
    }
    
    fn skip_whitespace(&mut self)  {
        while self.current.token_type == ShTokenType::WhiteSpace {
            self.next_token();
        }
    }

    fn parse_quoted_string(&mut self) -> String {
        let mut ret: String = String::from("");
        self.next_token();
        while self.current.token_type != ShTokenType::SingleQuote {
           ret.push_str(&self.current.lexeme);
           self.next_token();
        }
        self.next_token(); // skip the trailing double quote
        self.skip_whitespace(); // skip any trailing whitespace
        ret
    }

    fn next_token(&mut self) {
        // this seems really wasteful but the borrow checker beat me up -- how do we change current 
        // and prev to be references?
        // println!("l: {} c: {:?}, p: {:?}", self.loc, self.current, self.prev);
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
