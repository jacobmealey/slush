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
    pub err: String,
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
            err: "".to_string(),
        };
        if !parser.token.is_empty() {
            parser.current = parser.token[0].clone();
        }
        parser
    }

    pub fn parse(&mut self) {
        self.err = "".to_string();
        while self.current.token_type != ShTokenType::EndOfFile {
            match self.parse_andor_list() {
                Ok(expr) => { self.exprs.push(expr) },
                Err(strn) => { self.err += &strn; }
            };
            self.next_token(); // skip a newline
        }
    }

    // the results are a left-associative no precedence 
    // list of and / or expressions.  
    fn parse_andor_list(&mut self) -> Result<AndOrNode, String> {
        let mut left = AndOrNode::Pipeline(Box::new(self.parse_pipeline()?));
        while self.current_is(ShTokenType::AndIf) || self.current_is(ShTokenType::OrIf) {
            if self.current_is(ShTokenType::AndIf) {
                self.consume(ShTokenType::AndIf);
                let right = AndOrNode::Pipeline(Box::new(self.parse_pipeline()?));
                left = AndOrNode::Andif(Box::new(AndIf{left, right}));
            }
            // these feels yucky - how do we get these two nearly identical blocks 
            self.skip_whitespace();
            if self.current_is(ShTokenType::OrIf) {
                self.consume(ShTokenType::OrIf);
                self.skip_whitespace();
                let right = AndOrNode::Pipeline(Box::new(self.parse_pipeline()?));
                left = AndOrNode::Orif(Box::new(OrIf{left, right}));
            }
        }
        Ok(left)
    }

    fn parse_pipeline(&mut self) -> Result<PipeLineExpr, String> {
        let mut pipeline: Vec<CommandExpr> = Vec::new();
        pipeline.push(self.parse_command()?);

        while self.current.token_type == ShTokenType::Pipe && 
        !self.current_is(ShTokenType::NewLine) {
            self.consume(ShTokenType::Pipe);
            pipeline.push(self.parse_command()?);
        }
        Ok(PipeLineExpr { pipeline, capture_out: None })
    }

    fn parse_command(&mut self) -> Result<CommandExpr, String> {
        self.skip_whitespace();
        let assignment = self.parse_assignment()?;
        let mut err: String = "".to_string();
        let command_name = match self.parse_argument()? {
            Some(a) => a,
            None => {
                err = format!(
                    "Syntax error: Expected some command, instead found '{:?}'.",
                    self.current
                );
                Argument::Name("".to_string())
            }
        };

        if err.is_empty() && assignment.is_some() {
            return Err(err);
        }
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
            match self.parse_argument()? {
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
    fn parse_assignment(&mut self) -> Result<Option<AssignmentExpr>, String> {
        let current_location = self.loc;
        let mut key: String = String::from("");
        let mut val: Option<Argument> = None;
        if self.current.token_type == ShTokenType::Name {
            key = self.current.lexeme.clone();
            self.next_token();
            if self.current.token_type == ShTokenType::Equal {
                self.next_token();
                // an assignment can be a string, an @VAR or a direct token
                val = Some(match self.parse_argument()? {
                    Some(a) => a,
                    None => Argument::Name(String::from("")),
                });
            }
        }
        self.next_token();
        if let Some(argtype) = val {
            self.skip_whitespace();
            return Ok(Some(AssignmentExpr { key, val: argtype }));
        } else if current_location < self.token.len() {
            self.loc = current_location;
            self.current = self.token[self.loc].clone();
        } else {
            return Err(format!("Syntax error: Unexpected end of file after '{}'",
                        self.prev.lexeme));
        }
        self.skip_whitespace();
        Ok(None)
    }

    // Arguments can be A single quoteless string (Name), and quoted string or
    // a dollar sign var. so you could do:
    //   $ ls /tmp
    //   $ ls '/tmp'
    //   $ ls $TEMP_DIR
    fn parse_argument(&mut self) -> Result<Option<Argument>, String> {
        self.skip_whitespace();
        match self.current.token_type {
            ShTokenType::Name => Ok(Some(Argument::Name(self.current.lexeme.clone()))),
            ShTokenType::SingleQuote => Ok(Some(Argument::Name(self.parse_quoted_string()?))),
            ShTokenType::DollarSign => {
                self.next_token();
                Ok(Some(Argument::Variable(VariableLookup {
                    name: self.current.lexeme.clone(),
                })))
            }
            // this logic is not right - and breaks if you do something like:
            //      `echo `which ls``
            ShTokenType::BackTick => Ok(Some(Argument::SubShell(SubShellExpr {
                shell: self.collect_until(ShTokenType::BackTick)?,
            }))),
            _ => Ok(None),
        }
    }

    fn skip_whitespace(&mut self) {
        while self.current.token_type == ShTokenType::WhiteSpace {
            self.next_token();
        }
    }

    // On a single quote string we want to read every lexeme regardless
    // of the token type until we see another single quote.
    fn parse_quoted_string(&mut self) -> Result<String, String> {
        match self.collect_until(ShTokenType::SingleQuote) {
            Ok(s) => Ok(s),
            Err(_) => Err("Syntax error: unterminated string".to_string()),
        }
    }

    fn collect_until(&mut self, end: ShTokenType) -> Result<String, String> {
        let mut ret: String = String::from("");
        self.next_token();
        while self.current.token_type != end && self.current.token_type != ShTokenType::EndOfFile {
            ret.push_str(&self.current.lexeme);
            self.next_token();
        }
        if self.current.token_type == ShTokenType::EndOfFile {
            return Err(format!(
                    "Syntax error: Unexpected end of file after '{}', expected some token {:?}",
                    self.prev.lexeme, end));
        }
        self.skip_whitespace(); // skip any trailing whitespace
        Ok(ret)
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
            if self.loc > 0 && self.loc - 1 < self.token.len() {
                self.prev = self.token[self.loc - 1].clone();
            }
            self.current = Token {
                lexeme: "".to_string(),
                token_type: ShTokenType::EndOfFile,
            };
        } else {
            self.current = self.token[self.loc].clone();
            if self.loc > 0 {
                self.prev = self.token[self.loc - 1].clone();
            }
        }
    }
}


mod test {
    #[allow(unused_imports)]
    use super::*;
    #[allow(unused_imports)]
    use crate::parser::Parser;
    #[test]
    fn basic_command() {
        let line = "ls /var /tmp";
        let mut parser = Parser::new(&line);
        let golden_set = Vec::from([
            AndOrNode::Pipeline(
                Box::new(PipeLineExpr {
                    pipeline: Vec::from([
                      CommandExpr { 
                          command: Argument::Name("ls".to_string()),
                          arguments: Vec::from([
                              Argument::Name("/var".to_string()),
                              Argument::Name("/tmp".to_string())
                          ]),
                          assignment: None
                      }
                    ]),
                    capture_out: None
                }
            ))
        ]);
        parser.parse();
        for (i, expr) in golden_set.into_iter().enumerate() {
            assert!(parser.exprs[i].eq(&expr));
        }
    }

    #[test]
    fn test_only_ls() {
        let line = "ls";
        let mut parser = Parser::new(&line);
        let golden_set = Vec::from([
            AndOrNode::Pipeline(
                Box::new(PipeLineExpr {
                    pipeline: Vec::from([
                      CommandExpr { 
                          command: Argument::Name("ls".to_string()),
                          arguments: Vec::new(),
                          assignment: None
                      }
                    ]),
                    capture_out: None
                }
            ))
        ]);
        parser.parse();
        for (i, expr) in golden_set.into_iter().enumerate() {
            assert!(parser.exprs[i].eq(&expr));
        }
    }

    #[test]
    fn test_ls_pipe_wc() {
        let line = "ls | wc";
        let mut parser = Parser::new(&line);
        let golden_set = Vec::from([
            AndOrNode::Pipeline(
                Box::new(PipeLineExpr {
                    pipeline: Vec::from([
                      CommandExpr { 
                          command: Argument::Name("ls".to_string()),
                          arguments: Vec::new(),
                          assignment: None
                      },
                      CommandExpr { 
                          command: Argument::Name("wc".to_string()),
                          arguments: Vec::new(),
                          assignment: None
                      }
                    ]),
                    capture_out: None
                }
            ))
        ]);
        parser.parse();
        for (i, expr) in golden_set.into_iter().enumerate() {
            assert!(parser.exprs[i].eq(&expr));
        }
    }

    #[test]
    fn unexpected_eof() {
        let line = "ls |";
        let mut parser = Parser::new(&line);
        parser.parse();
        // We don't care what the error is just that there is one
        assert!(!parser.err.is_empty());
        assert_eq!(parser.exprs.len(), 0);
    }

    #[test]
    fn unterminated_string() {
        let line = "ls '";
        let mut parser = Parser::new(&line);
        parser.parse();
        // We don't care what the error is just that there is one
        assert!(!parser.err.is_empty());
        assert_eq!(parser.exprs.len(), 0);
    }
}
