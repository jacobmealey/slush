pub mod tokenizer;
use crate::expr::{
    AndIf, AndOrNode, Argument, AssignmentExpr, CommandExpr, CompoundList, ExpansionExpr, IfBranch,
    IfExpr, MergeExpr, OrIf, PipeLineExpr, State, SubShellExpr, VariableLookup,
};
use std::sync::{Arc, Mutex};

use crate::tokenizer::{tokens, ShTokenType, Token};

pub struct Parser {
    token: Vec<Token>,
    pub exprs: Vec<AndOrNode>,
    current: Token,
    prev: Token,
    loc: usize,
    pub err: String,
    state: Arc<Mutex<State>>,
}

impl Parser {
    pub fn new(state: Arc<Mutex<State>>) -> Parser {
        Parser {
            token: Vec::new(),
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
            state,
        }
    }

    pub fn parse(&mut self, line: &str) {
        self.err = "".to_string();
        self.token = match tokens(line) {
            Ok(t) => t,
            Err(e) => {
                self.err += &e;
                Vec::new()
            }
        };
        if !self.token.is_empty() {
            self.current = self.token[0].clone();
        }
        while self.current.token_type != ShTokenType::EndOfFile {
            match self.parse_andor_list() {
                Ok(expr) => self.exprs.push(expr),
                Err(strn) => {
                    self.err += &strn;
                }
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
                self.consume(ShTokenType::AndIf)?;
                let right = AndOrNode::Pipeline(Box::new(self.parse_pipeline()?));
                left = AndOrNode::Andif(Box::new(AndIf { left, right }));
            }
            // these feels yucky - how do we get these two nearly identical blocks
            if self.current_is(ShTokenType::OrIf) {
                self.consume(ShTokenType::OrIf)?;
                self.skip_whitespace();
                let right = AndOrNode::Pipeline(Box::new(self.parse_pipeline()?));
                left = AndOrNode::Orif(Box::new(OrIf { left, right }));
            }
        }
        self.skip_whitespace();
        Ok(left)
    }

    fn parse_pipeline(&mut self) -> Result<PipeLineExpr, String> {
        self.skip_whitespace();
        let mut pipeline: Vec<CompoundList> = Vec::new();
        pipeline.push(match self.current.token_type {
            ShTokenType::If => CompoundList::Ifexpr(self.parse_if()?),
            //ShTokenType::Function => self.parse_function()?,
            _ => CompoundList::Commandexpr(self.parse_command()?),
        });
        while self.current_is(ShTokenType::Pipe) {
            self.consume(ShTokenType::Pipe)?;
            pipeline.push(match self.current.token_type {
                ShTokenType::If => CompoundList::Ifexpr(self.parse_if()?),
                //ShTokenType::Function => self.parse_function()?,
                _ => CompoundList::Commandexpr(self.parse_command()?),
            });
        }
        self.skip_whitespace();
        let file_redirect = self.parse_redirect()?;
        let background = self.parse_control();

        Ok(PipeLineExpr {
            pipeline,
            capture_out: None,
            file_redirect,
            background,
            state: self.state.clone(),
        })
    }

    // parse_if builds out entire if/elif/else chain.
    fn parse_if(&mut self) -> Result<IfExpr, String> {
        if self.current_is(ShTokenType::If) {
            self.consume(ShTokenType::If)?;
        } else if self.current_is(ShTokenType::Elif) {
            self.consume(ShTokenType::Elif)?;
        }
        let condition = self.parse_pipeline()?;
        self.consume(ShTokenType::SemiColon)?;
        self.consume(ShTokenType::Then)?;
        self.skip_whitespace_newlines();
        let mut if_branch: Vec<PipeLineExpr> = Vec::new();
        let mut else_branch: Option<IfBranch> = None;
        while !self.current_is(ShTokenType::Fi)
            && !self.current_is(ShTokenType::Else)
            && !self.current_is(ShTokenType::Elif)
        {
            if_branch.push(self.parse_pipeline()?);
            self.next_token();
        }

        self.skip_whitespace_newlines();
        if self.current_is(ShTokenType::Elif) {
            else_branch = Some(IfBranch::Elif(Box::new(self.parse_if()?)));
        } else if self.current_is(ShTokenType::Else) {
            self.consume(ShTokenType::Else)?;
            let mut commands: Vec<PipeLineExpr> = Vec::new();
            while !self.current_is(ShTokenType::Fi) {
                self.skip_whitespace_newlines();
                commands.push(self.parse_pipeline()?);
                self.next_token();
            }
            else_branch = Some(IfBranch::Else(commands));
        }

        Ok(IfExpr {
            condition,
            if_branch,
            else_branch,
        })
    }

    // fn parse_function(&mut self) -> Result<FunctionExpr, String> {

    // }

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
            && self.current.token_type != ShTokenType::RedirectOut
            && self.current.token_type != ShTokenType::Control
        {
            self.next_token();
            // how do generalize this?
            match self.parse_argument()? {
                Some(a) => {
                    if !command.arguments.is_empty()
                        && self.prev.token_type != ShTokenType::WhiteSpace
                    {
                        let l = command.arguments.pop().unwrap();
                        command.arguments.push(Argument::Merge(MergeExpr {
                            left: Box::new(l),
                            right: Box::new(a),
                        }));
                    } else {
                        command.arguments.push(a)
                    }
                }
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
            return Err(format!(
                "Syntax error: Unexpected end of file after {:?}",
                self.prev.lexeme
            ));
        }
        self.skip_whitespace();
        Ok(None)
    }

    fn parse_expansion(&mut self) -> Result<ExpansionExpr, String> {
        self.next_token();
        if self.current_is(ShTokenType::Pound) {
            println!("huh?");
            self.next_token();
            if !self.current_is(ShTokenType::Name) {
                return Err(String::from("Expected a name after '#'"));
            }

            return Ok(ExpansionExpr::StringLengthExpansion(
                self.current.lexeme.clone(),
            ));
        } else if self.current_is(ShTokenType::Name) {
            // we are doing some type expansion thiny
            let name = self.current.lexeme.clone();
            self.next_token();
            if self.current_is(ShTokenType::UseDefault) {
                self.next_token();
                let default = self.collect_until(ShTokenType::RightBrace)?;
                return Ok(ExpansionExpr::ParameterSubstitute(name, default));
            } else if self.current_is(ShTokenType::AssignDefault) {
                self.next_token();
                let default = self.collect_until(ShTokenType::RightBrace)?;
                return Ok(ExpansionExpr::ParameterAssign(name, default));
            } else if self.current_is(ShTokenType::ErrorOn) {
                self.next_token();
                let default = self.collect_until(ShTokenType::RightBrace)?;
                return Ok(ExpansionExpr::ParameterError(name, default));
            } else if self.current_is(ShTokenType::UseNullOrDefault) {
                self.next_token();
                let default = self.collect_until(ShTokenType::RightBrace)?;
                return Ok(ExpansionExpr::ParameterExpansion(default));
            } else {
                let default = self.collect_until(ShTokenType::RightBrace)?;
                return Ok(ExpansionExpr::ParameterExpansion(default));
            }
        }

        self.consume(ShTokenType::RightBrace)?;
        Err(String::from("Error parsing expansion"))
    }

    fn parse_redirect(&mut self) -> Result<Option<Argument>, String> {
        self.skip_whitespace();
        if self.current_is(ShTokenType::RedirectOut) {
            self.consume(ShTokenType::RedirectOut)?;
            let filename = self.parse_argument()?;
            return Ok(filename);
        }
        Ok(None)
    }

    fn parse_control(&mut self) -> bool {
        self.skip_whitespace();
        if self.current_is(ShTokenType::Control) {
            let _ = self.consume(ShTokenType::Control);
            return true;
        }
        false
    }

    // Arguments can be A single quoteless string (Name), and quoted string or
    // a dollar sign var. so you could do:
    //   $ ls /tmp
    //   $ ls '/tmp'
    //   $ ls $TEMP_DIR
    //   Made public so the tokenizer can use it?
    pub fn parse_argument(&mut self) -> Result<Option<Argument>, String> {
        self.skip_whitespace();
        match self.current.token_type {
            ShTokenType::Name => Ok(Some(Argument::Name(self.current.lexeme.clone()))),
            ShTokenType::DollarSign => {
                self.next_token();
                match self.current.token_type {
                    ShTokenType::Name => Ok(Some(Argument::Variable(VariableLookup {
                        name: self.current.lexeme.clone(),
                    }))),
                    ShTokenType::LeftParen => Ok(Some(Argument::SubShell(SubShellExpr {
                        shell: self
                            .collect_matching(ShTokenType::LeftParen, ShTokenType::RightParen)?,
                    }))),
                    ShTokenType::LeftBrace => {
                        Ok(Some(Argument::Expansion(self.parse_expansion()?)))
                    }
                    _ => Err("Expected some value after '$'".to_string()),
                }
            }
            ShTokenType::BackTickStr => Ok(Some(Argument::SubShell(SubShellExpr {
                shell: self.current.lexeme.clone(),
            }))),
            // This is wildly ugly -- someone make this better!
            // We must do this in order to detect 'if' or 'else' as arguments and
            // translate the individual lexeme to a named argument
            ShTokenType::Fi
            | ShTokenType::Else
            | ShTokenType::Elif
            | ShTokenType::If
            | ShTokenType::Do
            | ShTokenType::For
            | ShTokenType::While
            | ShTokenType::Function
            | ShTokenType::Case
            | ShTokenType::Esac
            | ShTokenType::Then => Ok(Some(Argument::Name(self.current.lexeme.clone()))),
            _ => Ok(None),
        }
    }

    fn skip_whitespace(&mut self) {
        while self.current_is(ShTokenType::WhiteSpace) {
            self.next_token();
        }
    }

    fn skip_whitespace_newlines(&mut self) {
        while self.current_is(ShTokenType::WhiteSpace) || self.current_is(ShTokenType::NewLine) {
            self.next_token();
        }
    }

    // For braces and parents assumes you have already ingested left
    fn collect_matching(
        &mut self,
        left: ShTokenType,
        right: ShTokenType,
    ) -> Result<String, String> {
        let mut count = 1;
        let mut ret: String = String::new();
        while count > 0 && !self.current_is(right) {
            self.next_token();
            if self.current_is(ShTokenType::EndOfFile) {
                return Err(format!(
                    "Syntax Error: Unexpected end of file, no matching '{:?}'",
                    right
                ));
            }
            count += if self.current.token_type == left {
                1
            } else if self.current.token_type == right {
                -1
            } else {
                0
            };
            if count > 0 {
                ret.push_str(&self.current.lexeme)
            }
        }
        Ok(ret)
    }

    fn collect_until(&mut self, stop: ShTokenType) -> Result<String, String> {
        let mut ret: String = String::new();
        while !self.current_is(stop) {
            if self.current_is(ShTokenType::EndOfFile) {
                return Err(format!(
                    "Syntax Error: Unexpected end of file, no matching '{:?}'",
                    stop
                ));
            }
            ret.push_str(&self.current.lexeme);
            self.next_token();
        }
        Ok(ret)
    }

    fn current_is(&self, check: ShTokenType) -> bool {
        self.current.token_type == check
    }

    fn consume(&mut self, token: ShTokenType) -> Result<(), String> {
        self.skip_whitespace();
        if self.current_is(token) {
            self.next_token();
            Ok(())
        } else {
            Err(format!(
                "Syntax error: Expected a token {:?}, but found {:?}",
                token, self.current.lexeme
            ))
        }
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
    use crate::expr;
    #[allow(unused_imports)]
    use crate::parser::Parser;
    #[test]
    fn basic_command() {
        let line = "ls /var /tmp";
        let state = expr::State::new();
        let mut parser = Parser::new(state);
        let golden_set = Vec::from([AndOrNode::Pipeline(Box::new(PipeLineExpr {
            pipeline: Vec::from([CompoundList::Commandexpr(CommandExpr {
                command: Argument::Name("ls".to_string()),
                arguments: Vec::from([
                    Argument::Name("/var".to_string()),
                    Argument::Name("/tmp".to_string()),
                ]),
                assignment: None,
            })]),
            capture_out: None,
            file_redirect: None,
            background: false,
            state: expr::State::new(),
        }))]);
        parser.parse(&line);
        for (i, expr) in golden_set.into_iter().enumerate() {
            assert!(parser.exprs[i].eq(&expr));
        }
    }

    #[test]
    fn test_only_ls() {
        let line = "ls";

        let state = expr::State::new();
        let mut parser = Parser::new(state);
        let golden_set = Vec::from([AndOrNode::Pipeline(Box::new(PipeLineExpr {
            pipeline: Vec::from([CompoundList::Commandexpr(CommandExpr {
                command: Argument::Name("ls".to_string()),
                arguments: Vec::new(),
                assignment: None,
            })]),
            capture_out: None,
            file_redirect: None,
            background: false,
            state: expr::State::new(),
        }))]);
        parser.parse(&line);
        for (i, expr) in golden_set.into_iter().enumerate() {
            assert!(parser.exprs[i].eq(&expr));
        }
    }

    #[test]
    fn test_ls_pipe_wc() {
        let line = "ls | wc";
        let state = expr::State::new();
        let mut parser = Parser::new(state);
        let golden_set = Vec::from([AndOrNode::Pipeline(Box::new(PipeLineExpr {
            pipeline: Vec::from([
                CompoundList::Commandexpr(CommandExpr {
                    command: Argument::Name("ls".to_string()),
                    arguments: Vec::new(),
                    assignment: None,
                }),
                CompoundList::Commandexpr(CommandExpr {
                    command: Argument::Name("wc".to_string()),
                    arguments: Vec::new(),
                    assignment: None,
                }),
            ]),
            capture_out: None,
            file_redirect: None,
            background: false,
            state: expr::State::new(),
        }))]);
        parser.parse(&line);
        for (i, expr) in golden_set.into_iter().enumerate() {
            assert!(parser.exprs[i].eq(&expr));
        }
    }

    #[test]
    fn unexpected_eof() {
        let line = "ls |";

        let state = expr::State::new();
        let mut parser = Parser::new(state);
        parser.parse(&line);
        // We don't care what the error is just that there is one
        assert!(!parser.err.is_empty());
        assert_eq!(parser.exprs.len(), 0);
    }

    #[test]
    fn unterminated_string() {
        let line = "ls '";

        let state = expr::State::new();
        let mut parser = Parser::new(state);
        parser.parse(&line);
        // We don't care what the error is just that there is one
        assert!(!parser.err.is_empty());
        assert_eq!(parser.exprs.len(), 0);
    }

    #[test]
    fn happy_path_subshell() {
        let line = "echo `which ls`";

        let state = expr::State::new();
        let mut parser = Parser::new(state);
        let golden_set = Vec::from([AndOrNode::Pipeline(Box::new(PipeLineExpr {
            pipeline: Vec::from([CompoundList::Commandexpr(CommandExpr {
                command: Argument::Name("echo".to_string()),
                arguments: Vec::from([Argument::SubShell(SubShellExpr {
                    shell: "which ls".to_string(),
                })]),
                assignment: None,
            })]),
            capture_out: None,
            file_redirect: None,
            background: false,
            state: expr::State::new(),
        }))]);
        parser.parse(&line);
        for (i, expr) in golden_set.into_iter().enumerate() {
            assert!(parser.exprs[i].eq(&expr));
        }
    }

    #[test]
    fn undelimited_subshell() {
        let line = "ls `";
        let state = expr::State::new();
        let mut parser = Parser::new(state);
        parser.parse(&line);
        // We don't care what the error is just that there is one
        assert!(!parser.err.is_empty());
        assert_eq!(parser.exprs.len(), 0);
    }

    #[test]
    fn multi_line_command() {
        let line = "echo 'hello world' \n echo 'goodbye world'";
        let state = expr::State::new();
        let mut parser = Parser::new(state);
        let golden_set = Vec::from([
            AndOrNode::Pipeline(Box::new(PipeLineExpr {
                pipeline: Vec::from([CompoundList::Commandexpr(CommandExpr {
                    command: Argument::Name("echo".to_string()),
                    arguments: Vec::from([Argument::Name("hello world".to_string())]),
                    assignment: None,
                })]),
                capture_out: None,
                file_redirect: None,
                background: false,
                state: expr::State::new(),
            })),
            AndOrNode::Pipeline(Box::new(PipeLineExpr {
                pipeline: Vec::from([CompoundList::Commandexpr(CommandExpr {
                    command: Argument::Name("echo".to_string()),
                    arguments: Vec::from([Argument::Name("goodbye world".to_string())]),
                    assignment: None,
                })]),
                capture_out: None,
                file_redirect: None,
                background: false,
                state: expr::State::new(),
            })),
        ]);
        parser.parse(&line);
        assert!(parser.err.is_empty());
        for (i, expr) in golden_set.into_iter().enumerate() {
            assert!(parser.exprs[i].eq(&expr));
        }
    }

    #[test]
    fn tes_ls_and_pwd() {
        let line = "ls && pwd";
        let state = expr::State::new();
        let mut parser = Parser::new(state);
        let golden_set = Vec::from([AndOrNode::Andif(Box::new(AndIf {
            left: AndOrNode::Pipeline(Box::new(PipeLineExpr {
                pipeline: Vec::from([CompoundList::Commandexpr(CommandExpr {
                    command: Argument::Name("ls".to_string()),
                    arguments: Vec::new(),
                    assignment: None,
                })]),
                capture_out: None,
                file_redirect: None,
                background: false,
                state: expr::State::new(),
            })),
            right: AndOrNode::Pipeline(Box::new(PipeLineExpr {
                pipeline: Vec::from([CompoundList::Commandexpr(CommandExpr {
                    command: Argument::Name("pwd".to_string()),
                    arguments: Vec::new(),
                    assignment: None,
                })]),
                capture_out: None,
                file_redirect: None,
                background: false,
                state: expr::State::new(),
            })),
        }))]);
        parser.parse(&line);
        assert!(parser.err.is_empty());
        for (i, expr) in golden_set.into_iter().enumerate() {
            assert!(parser.exprs[i].eq(&expr));
        }
    }

    #[test]
    fn subshell_subshell() {
        let line = "echo $(echo $(echo 'hello world'))";
        let state = expr::State::new();
        let mut parser = Parser::new(state);
        let golden_set = Vec::from([AndOrNode::Pipeline(Box::new(PipeLineExpr {
            pipeline: Vec::from([CompoundList::Commandexpr(CommandExpr {
                command: Argument::Name("echo".to_string()),
                arguments: Vec::from([Argument::SubShell(SubShellExpr {
                    shell: "echo $(echo hello world)".to_string(),
                })]),
                assignment: None,
            })]),
            capture_out: None,
            file_redirect: None,
            background: false,
            state: expr::State::new(),
        }))]);
        parser.parse(&line);
        println!("{:?}", parser.exprs);
        assert!(parser.err.is_empty());
        for (i, expr) in golden_set.into_iter().enumerate() {
            assert!(parser.exprs[i].eq(&expr));
        }
    }

    #[test]
    fn test_if_statement() {
        let line = "if true; then echo 'hello world'\nfi";
        let state = expr::State::new();
        let mut parser = Parser::new(state);
        let golden_set = Vec::from([AndOrNode::Pipeline(Box::new(PipeLineExpr {
            pipeline: Vec::from([CompoundList::Ifexpr(IfExpr {
                condition: PipeLineExpr {
                    pipeline: Vec::from([CompoundList::Commandexpr(CommandExpr {
                        command: Argument::Name("true".to_string()),
                        arguments: Vec::new(),
                        assignment: None,
                    })]),
                    capture_out: None,
                    file_redirect: None,
                    background: false,
                    state: expr::State::new(),
                },
                if_branch: Vec::from([PipeLineExpr {
                    pipeline: Vec::from([CompoundList::Commandexpr(CommandExpr {
                        command: Argument::Name("echo".to_string()),
                        arguments: Vec::from([Argument::Name("hello world".to_string())]),
                        assignment: None,
                    })]),
                    capture_out: None,
                    file_redirect: None,
                    background: false,
                    state: expr::State::new(),
                }]),
                else_branch: None,
            })]),
            capture_out: None,
            file_redirect: None,
            background: false,
            state: expr::State::new(),
        }))]);
        parser.parse(&line);
        assert!(parser.err.is_empty());
        for (i, expr) in golden_set.into_iter().enumerate() {
            assert!(parser.exprs[i].eq(&expr));
        }
    }

    #[test]
    fn test_mergeable_line_with_backtick() {
        let line = "echo hello`world`";
        let state = expr::State::new();
        let mut parser = Parser::new(state);
        let golden_set = Vec::from([AndOrNode::Pipeline(Box::new(PipeLineExpr {
            pipeline: Vec::from([CompoundList::Commandexpr(CommandExpr {
                command: Argument::Name("echo".to_string()),
                arguments: Vec::from([Argument::Merge(MergeExpr {
                    left: Box::new(Argument::Name("hello".to_string())),
                    right: Box::new(Argument::SubShell(SubShellExpr {
                        shell: "world".to_string(),
                    })),
                })]),
                assignment: None,
            })]),
            capture_out: None,
            file_redirect: None,
            background: false,
            state: expr::State::new(),
        }))]);
        parser.parse(&line);
        assert!(parser.err.is_empty());
        for (i, expr) in golden_set.into_iter().enumerate() {
            assert!(parser.exprs[i].eq(&expr));
        }
    }

    #[test]
    fn test_mergeable_line_with_variable() {
        let line = "echo hello$PWD";
        let state = expr::State::new();
        let mut parser = Parser::new(state);
        let golden_set = Vec::from([AndOrNode::Pipeline(Box::new(PipeLineExpr {
            pipeline: Vec::from([CompoundList::Commandexpr(CommandExpr {
                command: Argument::Name("echo".to_string()),
                arguments: Vec::from([Argument::Merge(MergeExpr {
                    left: Box::new(Argument::Name("hello".to_string())),
                    right: Box::new(Argument::Variable(VariableLookup {
                        name: "PWD".to_string(),
                    })),
                })]),
                assignment: None,
            })]),
            capture_out: None,
            file_redirect: None,
            background: false,
            state: expr::State::new(),
        }))]);
        parser.parse(&line);
        assert!(parser.err.is_empty());
        for (i, expr) in golden_set.into_iter().enumerate() {
            assert!(parser.exprs[i].eq(&expr));
        }
    }

    #[test]
    fn test_mergeable_line_shell_first() {
        let line = "echo $(pwd)file";
        let state = expr::State::new();
        let mut parser = Parser::new(state);
        let golden_set = Vec::from([AndOrNode::Pipeline(Box::new(PipeLineExpr {
            pipeline: Vec::from([CompoundList::Commandexpr(CommandExpr {
                command: Argument::Name("echo".to_string()),
                arguments: Vec::from([Argument::Merge(MergeExpr {
                    left: Box::new(Argument::SubShell(SubShellExpr {
                        shell: "pwd".to_string(),
                    })),
                    right: Box::new(Argument::Name("file".to_string())),
                })]),
                assignment: None,
            })]),
            capture_out: None,
            file_redirect: None,
            background: false,
            state: expr::State::new(),
        }))]);
        parser.parse(&line);
        println!("{:?}", parser.exprs);
        println!("{:?}", golden_set);
        assert!(parser.err.is_empty());
        for (i, expr) in golden_set.into_iter().enumerate() {
            assert!(parser.exprs[i].eq(&expr));
        }
    }

    #[test]
    fn test_dangling_dollar_sign() {
        let line = "echo $";
        let state = expr::State::new();
        let mut parser = Parser::new(state);
        parser.parse(&line);
        assert!(!parser.err.is_empty());
    }

    #[test]
    fn test_dangling_dollar_sign_in_dangling_and_if() {
        let line = "echo $ &&";
        let state = expr::State::new();
        let mut parser = Parser::new(state);
        parser.parse(&line);
        assert!(!parser.err.is_empty());
    }

    #[test]
    fn test_if_else_statement() {
        let line = "if true; then\necho 'hello world'\nelse echo 'goodbye world'\nfi";
        let state = expr::State::new();
        let mut parser = Parser::new(state);
        let golden_set = Vec::from([AndOrNode::Pipeline(Box::new(PipeLineExpr {
            pipeline: Vec::from([CompoundList::Ifexpr(IfExpr {
                condition: PipeLineExpr {
                    pipeline: Vec::from([CompoundList::Commandexpr(CommandExpr {
                        command: Argument::Name("true".to_string()),
                        arguments: Vec::new(),
                        assignment: None,
                    })]),
                    capture_out: None,
                    file_redirect: None,
                    background: false,
                    state: expr::State::new(),
                },
                if_branch: Vec::from([PipeLineExpr {
                    pipeline: Vec::from([CompoundList::Commandexpr(CommandExpr {
                        command: Argument::Name("echo".to_string()),
                        arguments: Vec::from([Argument::Name("hello world".to_string())]),
                        assignment: None,
                    })]),
                    capture_out: None,
                    file_redirect: None,
                    background: false,
                    state: expr::State::new(),
                }]),
                else_branch: Some(IfBranch::Else(Vec::from([PipeLineExpr {
                    pipeline: Vec::from([CompoundList::Commandexpr(CommandExpr {
                        command: Argument::Name("echo".to_string()),
                        arguments: Vec::from([Argument::Name("goodbye world".to_string())]),
                        assignment: None,
                    })]),
                    capture_out: None,
                    file_redirect: None,
                    background: false,
                    state: expr::State::new(),
                }]))),
            })]),
            capture_out: None,
            file_redirect: None,
            background: false,
            state: expr::State::new(),
        }))]);
        parser.parse(&line);
        println!("{:#?}", parser.exprs);
        // println!("{:#?}", golden_set);
        assert!(parser.err.is_empty());
        for (i, expr) in golden_set.into_iter().enumerate() {
            assert!(parser.exprs[i].eq(&expr));
        }
    }

    #[test]
    fn test_valid_lexeme_as_argument_after_command() {
        let line = "echo if else";
        let state = expr::State::new();
        let mut parser = Parser::new(state);
        let golden_set = Vec::from([AndOrNode::Pipeline(Box::new(PipeLineExpr {
            pipeline: Vec::from([CompoundList::Commandexpr(CommandExpr {
                command: Argument::Name("echo".to_string()),
                arguments: Vec::from([
                    Argument::Name("if".to_string()),
                    Argument::Name("else".to_string()),
                ]),
                assignment: None,
            })]),
            capture_out: None,
            file_redirect: None,
            background: false,
            state: expr::State::new(),
        }))]);
        parser.parse(&line);
        println!("{:#?}", parser.exprs);
        // println!("{:#?}", golden_set);
        assert!(parser.err.is_empty());
        for (i, expr) in golden_set.into_iter().enumerate() {
            assert!(parser.exprs[i].eq(&expr));
        }
    }

    #[test]
    fn test_if_elif_else() {
        let line = "if true; then\n exit 1\nelif false; then\n exit 2\nelse\n exit 3\nfi";
        let state = expr::State::new();
        let mut parser = Parser::new(state);
        let golden_set = Vec::from([AndOrNode::Pipeline(Box::new(PipeLineExpr {
            pipeline: Vec::from([CompoundList::Ifexpr(IfExpr {
                condition: PipeLineExpr {
                    pipeline: Vec::from([CompoundList::Commandexpr(CommandExpr {
                        command: Argument::Name("true".to_string()),
                        arguments: Vec::new(),
                        assignment: None,
                    })]),
                    capture_out: None,
                    file_redirect: None,
                    background: false,
                    state: expr::State::new(),
                },
                if_branch: Vec::from([PipeLineExpr {
                    pipeline: Vec::from([CompoundList::Commandexpr(CommandExpr {
                        command: Argument::Name("exit".to_string()),
                        arguments: Vec::from([Argument::Name("1".to_string())]),
                        assignment: None,
                    })]),
                    capture_out: None,
                    file_redirect: None,
                    background: false,
                    state: expr::State::new(),
                }]),
                else_branch: Some(expr::IfBranch::Elif(Box::new(IfExpr {
                    condition: PipeLineExpr {
                        pipeline: Vec::from([CompoundList::Commandexpr(CommandExpr {
                            command: Argument::Name("false".to_string()),
                            arguments: Vec::new(),
                            assignment: None,
                        })]),
                        capture_out: None,
                        file_redirect: None,
                        background: false,
                        state: expr::State::new(),
                    },
                    if_branch: Vec::from([PipeLineExpr {
                        pipeline: Vec::from([CompoundList::Commandexpr(CommandExpr {
                            command: Argument::Name("exit".to_string()),
                            arguments: Vec::from([Argument::Name("2".to_string())]),
                            assignment: None,
                        })]),
                        capture_out: None,
                        file_redirect: None,
                        background: false,
                        state: expr::State::new(),
                    }]),
                    else_branch: Some(IfBranch::Else(Vec::from([PipeLineExpr {
                        pipeline: Vec::from([CompoundList::Commandexpr(CommandExpr {
                            command: Argument::Name("exit".to_string()),
                            arguments: Vec::from([Argument::Name("3".to_string())]),
                            assignment: None,
                        })]),
                        capture_out: None,
                        file_redirect: None,
                        background: false,
                        state: expr::State::new(),
                    }]))),
                }))),
            })]),
            capture_out: None,
            file_redirect: None,
            background: false,
            state: expr::State::new(),
        }))]);
        parser.parse(&line);
        println!("---- Parser Tree ----");
        println!("{:#?}", parser.exprs);
        println!("---- Golden Tree ----");
        println!("{:#?}", golden_set);
        //
        assert!(parser.err.is_empty());
        for (i, expr) in golden_set.into_iter().enumerate() {
            assert!(parser.exprs[i].eq(&expr));
        }
    }

    #[test]
    fn test_if_elif() {
        let line = "if true; then\n exit 1\nelif false; then\n exit 2\nfi";
        let state = expr::State::new();
        let mut parser = Parser::new(state);
        let golden_set = Vec::from([AndOrNode::Pipeline(Box::new(PipeLineExpr {
            pipeline: Vec::from([CompoundList::Ifexpr(IfExpr {
                condition: PipeLineExpr {
                    pipeline: Vec::from([CompoundList::Commandexpr(CommandExpr {
                        command: Argument::Name("true".to_string()),
                        arguments: Vec::new(),
                        assignment: None,
                    })]),
                    capture_out: None,
                    file_redirect: None,
                    background: false,
                    state: expr::State::new(),
                },
                if_branch: Vec::from([PipeLineExpr {
                    pipeline: Vec::from([CompoundList::Commandexpr(CommandExpr {
                        command: Argument::Name("exit".to_string()),
                        arguments: Vec::from([Argument::Name("1".to_string())]),
                        assignment: None,
                    })]),
                    capture_out: None,
                    file_redirect: None,
                    background: false,
                    state: expr::State::new(),
                }]),
                else_branch: Some(IfBranch::Elif(Box::new(IfExpr {
                    condition: PipeLineExpr {
                        pipeline: Vec::from([CompoundList::Commandexpr(CommandExpr {
                            command: Argument::Name("false".to_string()),
                            arguments: Vec::new(),
                            assignment: None,
                        })]),
                        capture_out: None,
                        file_redirect: None,
                        background: false,
                        state: expr::State::new(),
                    },
                    if_branch: Vec::from([PipeLineExpr {
                        pipeline: Vec::from([CompoundList::Commandexpr(CommandExpr {
                            command: Argument::Name("exit".to_string()),
                            arguments: Vec::from([Argument::Name("2".to_string())]),
                            assignment: None,
                        })]),
                        capture_out: None,
                        file_redirect: None,
                        background: false,
                        state: expr::State::new(),
                    }]),
                    else_branch: None,
                }))),
            })]),
            capture_out: None,
            file_redirect: None,
            background: false,
            state: expr::State::new(),
        }))]);
        parser.parse(&line);
        println!("---- Parser Tree ----");
        println!("{:#?}", parser.exprs);
        println!("---- Golden Tree ----");
        println!("{:#?}", golden_set);
        //
        assert!(parser.err.is_empty());
        for (i, expr) in golden_set.into_iter().enumerate() {
            assert!(parser.exprs[i].eq(&expr));
        }
    }
}
