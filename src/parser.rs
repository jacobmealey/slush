pub mod tokenizer;
use crate::expr::Argument;
use crate::expr::AssignmentExpr;
use crate::expr::CommandExpr;
use crate::expr::PipeLineExpr;
use crate::expr::SubShellExpr;
use crate::expr::VariableLookup;
use crate::tokenizer::{tokens, ShTokenType, Token};
use crate::expr::AndIf;
use crate::expr::OrIf;
use crate::expr::AndOrNode;

pub struct Parser {
    token: Vec<Token>,
    pub exprs: Vec<AndOrNode>,
    current: Token,
    prev: Token,
    loc: usize,
}

impl Parser {
    pub fn new(line: &str) -> Parser {
        let mut parser = Parser {
            token: tokens(line),
            exprs: Vec::new(),
            current: Token {
                lexeme: "".to_string(),
                token_type: ShTokenType::EndOfFile,
            },
            prev: Token {
                lexeme: "".to_string(),
                token_type: ShTokenType::EndOfFile,
            },
            loc: 0,
        };
        if !parser.token.is_empty() {
            parser.current = parser.token[0].clone();
        }
        parser
    }

    pub fn parse(&mut self) {
        let expr = self.parse_andor_list();
        self.exprs.push(expr);
    }

    // the results are a left-associative no precedence 
    // list of and / or expressions. 
    fn parse_andor_list(&mut self) -> AndOrNode {
        let left = AndOrNode::Pipeline(Box::new(self.parse_pipeline()));
        if self.current_is(ShTokenType::AndIf) {
            self.consume(ShTokenType::AndIf);
            let right = self.parse_andor_list();
            return AndOrNode::Andif(Box::new(AndIf{left, right}));
        }
        // these feels yucky - how do we get these two nearly identical blocks 
        self.skip_whitespace();
        if self.current_is(ShTokenType::OrIf) {
            self.consume(ShTokenType::OrIf);
            self.skip_whitespace();
            let right = self.parse_andor_list();
            return AndOrNode::Orif(Box::new(OrIf{left, right}));
        }
        left
    }

    fn parse_pipeline(&mut self) -> PipeLineExpr {
        let mut pipeline: Vec<CommandExpr> = Vec::new();
        pipeline.push(match self.parse_command() {
            Ok(expr) => expr,
            Err(message) => {
                CommandExpr {
                    command: Argument::Name("echo".to_string()),
                    arguments: Vec::from([Argument::Name(message)]),
                    assignment: None,
                }
            }
        });
        while self.current.token_type == ShTokenType::Pipe {
            self.next_token();
            pipeline.push(match self.parse_command() {
                Ok(expr) => expr,
                Err(message) => {
                    CommandExpr {
                        command: Argument::Name("echo".to_string()),
                        arguments: Vec::from([Argument::Name(message)]),
                        assignment: None,
                    }   
                }
            });
        }
        PipeLineExpr { pipeline, capture_out: None }
    }

    fn parse_command(&mut self) -> Result<CommandExpr, String> {
        let assignment = self.parse_assignment();
        let command_name = match self.parse_argument() {
            Some(a) => a,
            None => {
                return Err(format!(
                    "Syntax error: Expected some command, instead found '{:?}'.",
                    self.current
                ));
            }
        };

        let mut command = CommandExpr {
            command: command_name,
            arguments: Vec::new(),
            assignment,
        };
        while self.current.token_type != ShTokenType::EndOfFile
            && self.current.token_type != ShTokenType::NewLine
            && self.current.token_type != ShTokenType::Pipe
            && self.current.token_type != ShTokenType::SemiColon
            && self.current.token_type != ShTokenType::AndIf
            && self.current.token_type != ShTokenType::OrIf
        {
            self.next_token();
            match self.parse_argument() {
                Some(a) => command.arguments.push(a),
                None => {
                    continue;
                } // ignore all tokens until a delimiting token
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
    fn parse_assignment(&mut self) -> Option<AssignmentExpr> {
        let current_location = self.loc;
        let mut key: String = String::from("");
        let mut val: Option<Argument> = None;
        if self.current.token_type == ShTokenType::Name {
            key = self.current.lexeme.clone();
            self.next_token();
            if self.current.token_type == ShTokenType::Equal {
                self.next_token();
                // an assignment can be a string, an @VAR or a direct token
                val = Some(match self.parse_argument() {
                    Some(a) => a,
                    None => Argument::Name(String::from("")),
                });
            }
        }
        if let Some(argtype) = val {
            self.skip_whitespace();
            return Some(AssignmentExpr { key, val: argtype });
        } else {
            self.loc = current_location;
            self.current = self.token[self.loc].clone();
        }
        self.skip_whitespace();
        None
    }

    // Arguments can be A single quoteless string (Name), and quoted string or
    // a dollar sign var. so you could do:
    //   $ ls /tmp
    //   $ ls '/tmp'
    //   $ ls $TEMP_DIR
    fn parse_argument(&mut self) -> Option<Argument> {
        self.skip_whitespace();
        match self.current.token_type {
            ShTokenType::Name => Some(Argument::Name(self.current.lexeme.clone())),
            ShTokenType::SingleQuote => Some(Argument::Name(self.parse_quoted_string())),
            ShTokenType::DollarSign => {
                self.next_token();
                Some(Argument::Variable(VariableLookup {
                    name: self.current.lexeme.clone(),
                }))
            }
            // this logic is not right - and breaks if you do something like:
            //      `echo `which ls``
            ShTokenType::BackTick => Some(Argument::SubShell(SubShellExpr {
                shell: self.collect_until(ShTokenType::BackTick),
            })),
            _ => None,
        }
    }

    fn skip_whitespace(&mut self) {
        while self.current.token_type == ShTokenType::WhiteSpace {
            self.next_token();
        }
    }

    // On a single quote string we want to read every lexeme regardless
    // of the token type until we see another single quote.
    fn parse_quoted_string(&mut self) -> String {
        self.collect_until(ShTokenType::SingleQuote)
    }

    fn collect_until(&mut self, end: ShTokenType) -> String {
        let mut ret: String = String::from("");
        self.next_token();
        while self.current.token_type != end && self.current.token_type != ShTokenType::EndOfFile {
            ret.push_str(&self.current.lexeme);
            self.next_token();
        }
        self.next_token(); // skip the trailing double quote
        self.skip_whitespace(); // skip any trailing whitespace
        ret
    }

    fn current_is(&self, check: ShTokenType) -> bool {
        self.current.token_type == check
    }

    fn consume(&mut self, token: ShTokenType) -> bool {
        if self.current_is(token) {
            self.next_token();
            return true
        }
        false
    }
    
    fn next_token(&mut self) {
        // this seems really wasteful but the borrow checker beat me up -- how do we change current
        // and prev to be references?
        // println!("l: {} c: {:?}, p: {:?}", self.loc, self.current, self.prev);
        self.loc += 1;
        if self.loc >= self.token.len() {
            self.current = Token {
                lexeme: "".to_string(),
                token_type: ShTokenType::EndOfFile,
            };
        } else {
            self.current = self.token[self.loc].clone();
            if self.loc > 1 {
                self.prev = self.token[self.loc - 1].clone();
            }
        }
    }
}
