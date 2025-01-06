pub mod tokenizer;
use crate::tokenizer::{
    Token,
    tokens,
    ShTokenType
};
use crate::expr::Evalable;
use crate::expr::CommandExpr;
use crate::expr::PipeLineExpr;
use crate::expr::AssignmentExpr;
use crate::expr::VariableLookup;
use crate::expr::Argument;
use crate::expr::Argument::*;


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
        let command_name = match self.parse_argument() {
            Some(a) => a,
            None => { return Err(
                format!(
                    "Syntax error: Expected some command, instead found '{:?}'.",
                    self.current
                ));        
            }
        };
        
        let mut command = CommandExpr {
            command: command_name,
            arguments: Vec::new()
        };
        while self.current.token_type != ShTokenType::EndOfFile &&
              self.current.token_type != ShTokenType::NewLine &&
              self.current.token_type != ShTokenType::Pipe {
            self.next_token();
            match self.parse_argument() {
                Some(a) => {command.arguments.push(a)},
                None => {continue;} // ignore all tokens until a delimiting token

            };
        }
        Ok(command)
    }

    // assignment expressions are optional at the beginning, it can be difficult
    // to tell if the assignment is a TRUE assignment until you get to an '=' sign
    // for example:
    // [0] $ VAR="Something"
    //     |----^
    // here VAR could be a valid standalone command, and we don't /know/ its an
    // assignment until we see the the '=' sign, if we don't we have to rewind to
    // the beginning. There must be a better way to do this?
    fn parse_assignment(&mut self) {
        let current_location = self.loc;
        let mut key: String = String::from("");
        let mut val: String = String::from("");
        if self.current.token_type == ShTokenType::Name {
            key = self.current.lexeme.clone();
            self.next_token();
            if self.current.token_type == ShTokenType::Equal {
                self.next_token();
                // an assignment can be a string, an @VAR or a direct token
                val = match self.parse_argument() {
                    Some(a) => {self.next_token(); match a {
                        Name(name) => name,
                        Variable(var) => var.name
                    }},
                    None => String::from("")
                };
            }
        }
        if !val.is_empty() {
            self.exprs.push(Box::new(AssignmentExpr{ key, val }));
        } else {
            self.loc = current_location;
            self.current =  self.token[self.loc].clone();
        }
        self.skip_whitespace();
    }

    // Arguments can be A single quoteless string (Name), and quoted string or
    // a dollar sign var. so you could do:
    //   $ ls /tmp
    //   $ ls '/tmp'
    //   $ ls $TEMP_DIR
    fn parse_argument(&mut self) -> Option<Argument>{
        self.skip_whitespace();
        if self.current.token_type == ShTokenType::Name {
            return Some(Argument::Name(self.current.lexeme.clone()));
        } else if self.current.token_type == ShTokenType::SingleQuote {
            return Some(Argument::Name(self.parse_quoted_string()));
        } else if self.current.token_type == ShTokenType::DollarSign {
            self.next_token();
            // return Some(env::var(self.current.lexeme.clone()).expect(""));
            return Some(Argument::Variable(VariableLookup {name: self.current.lexeme.clone()}))
        }
        None
    }
    
    fn skip_whitespace(&mut self)  {
        while self.current.token_type == ShTokenType::WhiteSpace {
            self.next_token();
        }
    }

    // On a single quote string we want to read every lexeme regardless
    // of the token type until we see another single quote.
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
