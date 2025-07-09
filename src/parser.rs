pub mod tokenizer;
use crate::expr::{
    AndIf, AndOrNode, Argument, AssignmentExpr, CommandExpr, CompoundList, ExpansionExpr, ForExpr,
    FunctionExpr, FunctionStack, IfBranch, IfExpr, MergeExpr, NotExpr, OrIf, PipeLineExpr,
    RedirectExpr, RedirectType, State, SubShellExpr, VariableLookup, WhileExpr,
};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::LazyLock;

use crate::tokenizer::{tokens, ShTokenType, Token};

static ENDOFFILE: LazyLock<Token> = LazyLock::new(|| Token {
    lexeme: "".to_string(),
    token_type: ShTokenType::EndOfFile,
});

pub struct Parser {
    token: Vec<Token>,
    pub exprs: Vec<AndOrNode>,
    loc: usize,
    pub err: String,
    state: Rc<RefCell<State>>,
}

impl Parser {
    pub fn new(state: Rc<RefCell<State>>) -> Parser {
        Parser {
            token: Vec::new(),
            exprs: Vec::new(),
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

        while !self.current_is(ShTokenType::EndOfFile) {
            match self.parse_andor_list() {
                Ok(expr) => self.exprs.push(expr),
                Err(strn) => {
                    self.err += &strn;
                }
            };
            self.try_consume(ShTokenType::NewLine);
        }
    }

    // the results are a left-associative no precedence
    // list of and / or expressions.
    fn parse_andor_list(&mut self) -> Result<AndOrNode, String> {
        self.skip_whitespace_newlines();
        let mut not = false;
        if self.try_consume(ShTokenType::Bang) {
            not = true;
        }
        let mut left = AndOrNode::Pipeline(Box::new(self.parse_pipeline()?));
        while self.current_is(ShTokenType::AndIf) || self.current_is(ShTokenType::OrIf) {
            if self.try_consume(ShTokenType::AndIf) {
                let right = AndOrNode::Pipeline(Box::new(self.parse_pipeline()?));
                left = AndOrNode::Andif(Box::new(AndIf { left, right }));
            }
            // these feels yucky - how do we get these two nearly identical blocks
            if self.try_consume(ShTokenType::OrIf) {
                self.skip_whitespace();
                let right = AndOrNode::Pipeline(Box::new(self.parse_pipeline()?));
                left = AndOrNode::Orif(Box::new(OrIf { left, right }));
            }
        }

        self.skip_whitespace_newlines();
        if not {
            left = AndOrNode::Notif(Box::new(NotExpr { condition: left }));
        }
        Ok(left)
    }

    fn parse_pipeline(&mut self) -> Result<PipeLineExpr, String> {
        self.skip_whitespace();
        let mut pipeline: Vec<CompoundList> = Vec::new();
        pipeline.push(match self.current().token_type {
            ShTokenType::If => CompoundList::Ifexpr(self.parse_if()?),
            ShTokenType::While | ShTokenType::Until => CompoundList::Whileexpr(self.parse_while()?),
            ShTokenType::For => CompoundList::Forexpr(self.parse_for()?),
            _ => {
                if let Some(function) = self.parse_function()? {
                    CompoundList::Functionexpr(function)
                } else {
                    CompoundList::Commandexpr(self.parse_command()?)
                }
            }
        });
        while self.try_consume(ShTokenType::Pipe) {
            pipeline.push(match self.current().token_type {
                ShTokenType::If => CompoundList::Ifexpr(self.parse_if()?),
                ShTokenType::While | ShTokenType::Until => {
                    CompoundList::Whileexpr(self.parse_while()?)
                }
                ShTokenType::For => CompoundList::Forexpr(self.parse_for()?),
                _ => CompoundList::Commandexpr(self.parse_command()?),
            });
        }

        let mut file_redirect = None;
        self.skip_whitespace();
        if !self.current_is(ShTokenType::EndOfFile) && !self.current_is(ShTokenType::NewLine) {
            file_redirect = self.parse_redirect()?;
        }
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
        if !self.try_consume(ShTokenType::If) {
            self.try_consume(ShTokenType::Elif);
        }
        let condition = self.parse_pipeline()?;
        self.skip_whitespace_newlines();
        self.consume(ShTokenType::Then)?;
        self.skip_whitespace_newlines();
        let mut if_branch: Vec<PipeLineExpr> = Vec::new();
        let mut else_branch: Option<IfBranch> = None;
        while !self.current_is(ShTokenType::Fi)
            && !self.current_is(ShTokenType::Else)
            && !self.current_is(ShTokenType::Elif)
        {
            if_branch.push(self.parse_pipeline()?);
            self.skip_whitespace_newlines();
        }

        self.skip_whitespace_newlines();
        // If its an elif then we can recursively parse the elif branch as
        // calling parse_if will consume the elif token. If its an else
        // branch we repeat the same process as the if branch
        if self.current_is(ShTokenType::Elif) {
            else_branch = Some(IfBranch::Elif(Box::new(self.parse_if()?)));
        } else if self.try_consume(ShTokenType::Else) {
            self.skip_whitespace_newlines();
            let mut commands: Vec<PipeLineExpr> = Vec::new();
            while !self.try_consume(ShTokenType::Fi) {
                commands.push(self.parse_pipeline()?);
                self.skip_whitespace_newlines();
            }
            else_branch = Some(IfBranch::Else(commands));
        }

        self.skip_whitespace_newlines();
        if else_branch.is_none() {
            self.consume(ShTokenType::Fi)?;
        }

        Ok(IfExpr {
            condition,
            if_branch,
            else_branch,
        })
    }

    fn parse_function(&mut self) -> Result<Option<FunctionExpr>, String> {
        let fname = self.loc; // dont copy function lexeme until we know we have a function
        self.next_token();
        if !self.try_consume(ShTokenType::LeftParen) {
            self.rewind(1);
            return Ok(None);
        }
        if !self.try_consume(ShTokenType::RightParen) {
            self.rewind(2);
            return Ok(None);
        }
        self.skip_whitespace_newlines();
        self.consume(ShTokenType::LeftBrace)?;
        self.skip_whitespace_newlines();
        let mut commands: Vec<PipeLineExpr> = Vec::new();
        while !self.try_consume(ShTokenType::RightBrace) {
            commands.push(self.parse_pipeline()?);
            self.skip_whitespace_newlines();
        }

        Ok(Some(FunctionExpr {
            fname: self.token[fname].lexeme.clone(),
            commands,
            args: FunctionStack::new(RefCell::new(Vec::new())),
        }))
    }

    fn parse_for(&mut self) -> Result<ForExpr, String> {
        self.consume(ShTokenType::For)?;
        self.skip_whitespace();
        if !self.current_is(ShTokenType::Name) {
            return Err(format!(
                "Expected some name after 'for' instead found {}",
                self.current().lexeme
            ));
        }
        let name = self.current().lexeme.clone();
        self.next_token();
        self.skip_whitespace();
        let list: Vec<Argument> = if self.try_consume(ShTokenType::In) {
            let mut vec: Vec<Argument> = Vec::new();
            while !(self.current_is(ShTokenType::SemiColon)
                || self.current_is(ShTokenType::NewLine)
                || self.current_is(ShTokenType::Do))
            {
                self.skip_whitespace();
                vec.push(match self.parse_argument()? {
                    Some(a) => a,
                    None => {
                        return Err(format!(
                            "Expected some argument instead found {}",
                            self.current().lexeme
                        ));
                    }
                });
            }
            self.try_consume(ShTokenType::SemiColon);
            self.skip_whitespace_newlines();
            self.consume(ShTokenType::Do)?;
            vec
        } else {
            Vec::new()
        };

        let mut commands: Vec<PipeLineExpr> = Vec::new();
        while !self.try_consume(ShTokenType::Done) {
            commands.push(self.parse_pipeline()?);
            self.skip_whitespace_newlines();
        }

        Ok(ForExpr {
            name,
            list,
            commands,
        })
    }

    fn parse_while(&mut self) -> Result<WhileExpr, String> {
        let mut not = false;
        if self.current_is(ShTokenType::Until) {
            not = true;
            self.next_token();
        } else if self.current_is(ShTokenType::While) {
            self.next_token();
        } else {
            return Err(format!(
                "Syntax error: Expected 'while' or 'until', found {:?}",
                self.current()
            ));
        }
        let mut condition = AndOrNode::Pipeline(Box::new(self.parse_pipeline()?));
        self.skip_whitespace_newlines();
        self.consume(ShTokenType::Do)?;
        self.skip_whitespace_newlines();
        let mut body: Vec<PipeLineExpr> = Vec::new();
        while !self.try_consume(ShTokenType::Done) {
            let vv = self.parse_pipeline()?;
            body.push(vv);
            self.skip_whitespace_newlines();
        }

        if not {
            condition = AndOrNode::Notif(Box::new(NotExpr { condition }));
        }

        Ok(WhileExpr { condition, body })
    }

    fn parse_command(&mut self) -> Result<CommandExpr, String> {
        let assignment = self.parse_assignment()?;
        let mut err: String = "".to_string();
        self.skip_whitespace();
        let command_name = match self.parse_argument()? {
            Some(a) => a,
            None => {
                err = format!(
                    "Syntax error: Expected some command, instead found '{:?}'.",
                    self.current()
                );
                Argument::Name("".to_string())
            }
        };

        if !err.is_empty() && assignment.is_some() {
            return Ok(CommandExpr {
                command: command_name,
                arguments: Rc::new(Vec::new()),
                assignment,
            });
        }
        let mut command = CommandExpr {
            command: command_name,
            arguments: Rc::new(Vec::new()),
            assignment,
        };

        self.skip_whitespace();
        while let Some(argument) = self.parse_argument()? {
            Rc::get_mut(&mut command.arguments).unwrap().push(argument);
            self.skip_whitespace();
        }
        self.try_consume(ShTokenType::SemiColon);
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
        let mut key: usize = 0;
        let mut val: Option<Argument> = None;
        if self.current_is(ShTokenType::Name) {
            key = self.loc;
            self.next_token(); // skip the key
            if !self.try_consume(ShTokenType::Equal) {
                // if we don't have an equal sign then we need to rewind
                // the token stream to the beginning of the assignment
                self.loc = current_location;
                return Ok(None);
            }
            val = self.parse_argument()?;
        }
        if let Some(argtype) = val {
            return Ok(Some(AssignmentExpr {
                key: self.token[key].lexeme.clone(),
                val: argtype,
            }));
        } else if current_location < self.token.len() {
            self.loc = current_location;
        } else {
            return Err(format!(
                "Syntax error: Unexpected end of file after {:?}",
                self.prev().lexeme
            ));
        }
        Ok(None)
    }

    fn parse_expansion(&mut self) -> Result<ExpansionExpr, String> {
        self.next_token();
        if self.current_is(ShTokenType::Pound) {
            self.next_token();
            if !self.current_is(ShTokenType::Name) {
                return Err(String::from("Expected a name after '#'"));
            }

            return Ok(ExpansionExpr::StringLengthExpansion(
                self.current().lexeme.clone(),
            ));
        } else if self.current_is(ShTokenType::Name) {
            // we are doing some type expansion thiny
            let name = self.current().lexeme.clone();
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
                return Ok(ExpansionExpr::ParameterExpansion(name));
            } else {
                self.consume(ShTokenType::RightBrace)?;
                return Ok(ExpansionExpr::ParameterExpansion(name));
            }
        }

        self.consume(ShTokenType::RightBrace)?;
        Err(String::from("Error parsing expansion"))
    }

    fn parse_redirect(&mut self) -> Result<Option<RedirectExpr>, String> {
        self.skip_whitespace();
        let mut file_descriptor = 1;
        let mode = if self.try_consume(ShTokenType::RedirectOut) {
            RedirectType::Out
        } else if self.try_consume(ShTokenType::AppendOut) {
            RedirectType::OutAppend
        } else if self.try_consume(ShTokenType::RedirectIn) {
            file_descriptor = 0;
            RedirectType::In
        } else {
            if self.current_is(ShTokenType::EndOfFile) {
                self.rewind(0);
            } else {
                self.rewind(1);
            }
            return Ok(None);
        };
        self.skip_whitespace();
        let file = match self.parse_argument()? {
            Some(a) => a,
            None => {
                return Err(format!(
                    "Syntax error: Expected a file name after '{:?}'",
                    self.current()
                ))
            }
        };
        Ok(Some(RedirectExpr {
            file,
            mode,
            file_descriptor,
        }))
    }

    fn parse_control(&mut self) -> bool {
        self.skip_whitespace();
        self.try_consume(ShTokenType::Control)
    }

    // Arguments can be A single quoteless string (Name), and quoted string or
    // a dollar sign var. so you could do:
    //   $ ls /tmp
    //   $ ls '/tmp'
    //   $ ls $TEMP_DIR
    //   Made public so the tokenizer can use it?
    pub fn parse_argument(&mut self) -> Result<Option<Argument>, String> {
        let mut argument: Option<Argument> = None;
        while let Some(a) = self._parse_argument()? {
            // while we still have tokens in the assignment we need
            // to construct it at run time by creating MergeExprss
            // of the various types of arguments strung together.
            if argument.is_some() {
                let l = argument.take().unwrap();
                argument = Some(Argument::Merge(MergeExpr {
                    left: Box::new(l),
                    right: Box::new(a),
                }));
            } else {
                argument = Some(a);
            }
        }
        Ok(argument)
    }

    // helper function for parse_argument that gets exactly one 'primitive' argument
    // which may be strung together to create
    fn _parse_argument(&mut self) -> Result<Option<Argument>, String> {
        match self.current().token_type {
            ShTokenType::Name => Ok(Some(Argument::Name(self.consume_current().lexeme.clone()))),
            ShTokenType::DollarSign => {
                self.consume(ShTokenType::DollarSign)?;
                match self.current().token_type {
                    ShTokenType::Name => Ok(Some(Argument::Variable(VariableLookup {
                        name: self.consume_current().lexeme.clone(),
                    }))),
                    ShTokenType::LeftParen => Ok(Some(Argument::SubShell(SubShellExpr {
                        shell: self
                            .collect_matching(ShTokenType::LeftParen, ShTokenType::RightParen)?,
                    }))),
                    ShTokenType::LeftBrace => {
                        Ok(Some(Argument::Expansion(self.parse_expansion()?)))
                    }
                    ShTokenType::DollarSign | ShTokenType::Bang | ShTokenType::Star => {
                        Ok(Some(Argument::Variable(VariableLookup {
                            name: self.consume_current().lexeme.clone(),
                        })))
                    }
                    _ => Err("Expected some value after '$'".to_string()),
                }
            }

            ShTokenType::BackTickStr => Ok(Some(Argument::SubShell(SubShellExpr {
                shell: self.consume_current().lexeme.clone(),
            }))),
            ShTokenType::WhiteSpace
            | ShTokenType::NewLine
            | ShTokenType::SemiColon
            | ShTokenType::AndIf
            | ShTokenType::Done
            | ShTokenType::OrIf
            | ShTokenType::Pipe
            | ShTokenType::RedirectOut
            | ShTokenType::AppendOut
            | ShTokenType::RedirectIn
            | ShTokenType::Control
            | ShTokenType::RightParen
            | ShTokenType::LeftParen
            | ShTokenType::EndOfFile => Ok(None),
            _ => Ok(Some(Argument::Name(self.consume_current().lexeme.clone()))),
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

    // For braces and parens assumes you have already ingested left
    fn collect_matching(
        &mut self,
        left: ShTokenType,
        right: ShTokenType,
    ) -> Result<String, String> {
        let mut count = 1;
        let mut ret: String = String::new();
        while count > 0 {
            self.next_token();
            if self.current_is(ShTokenType::EndOfFile) {
                return Err(format!(
                    "Syntax Error: Unexpected end of file, no matching '{right:?}'"
                ));
            }
            count += if self.current_is(left) {
                1
            } else if self.current_is(right) {
                -1
            } else {
                0
            };
            if count > 0 {
                ret.push_str(&self.current().lexeme)
            }
        }
        self.next_token();
        Ok(ret)
    }

    fn collect_until(&mut self, stop: ShTokenType) -> Result<String, String> {
        let mut ret: String = String::new();
        while !self.try_consume(stop) {
            if self.current_is(ShTokenType::EndOfFile) {
                return Err(format!(
                    "Syntax Error: Unexpected end of file, no matching '{stop:?}'"
                ));
            }
            ret.push_str(&self.current().lexeme);
            self.next_token();
        }
        Ok(ret)
    }

    fn current_is(&self, check: ShTokenType) -> bool {
        self.current().token_type == check
    }

    fn try_consume(&mut self, token: ShTokenType) -> bool {
        if self.current_is(token) {
            self.next_token();
            true
        } else {
            false
        }
    }

    fn consume(&mut self, token: ShTokenType) -> Result<(), String> {
        self.skip_whitespace();
        if self.current_is(token) {
            self.next_token();
            Ok(())
        } else {
            Err(format!(
                "Syntax error: Expected a token {:?}, but found {:?}",
                token,
                self.current().lexeme
            ))
        }
    }

    fn rewind(&mut self, n: usize) {
        self.loc = self.loc.saturating_sub(n);
    }

    fn consume_current(&mut self) -> &Token {
        self.next_token();
        self.prev()
    }

    fn next_token(&mut self) {
        // this seems really wasteful but the borrow checker beat me up -- how do we change current
        // and prev to be references?
        self.loc += 1;
    }

    fn current(&self) -> &Token {
        if self.loc < self.token.len() {
            &self.token[self.loc]
        } else {
            &ENDOFFILE
        }
    }

    fn prev(&self) -> &Token {
        if self.loc > 0 && self.loc < self.token.len() + 1 {
            &self.token[self.loc - 1]
        } else {
            &ENDOFFILE
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
                ])
                .into(),
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
                arguments: Vec::new().into(),
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
                    arguments: Vec::new().into(),
                    assignment: None,
                }),
                CompoundList::Commandexpr(CommandExpr {
                    command: Argument::Name("wc".to_string()),
                    arguments: Vec::new().into(),
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
                })])
                .into(),
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
                    arguments: Vec::from([Argument::Name("hello world".to_string())]).into(),
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
                    arguments: Vec::from([Argument::Name("goodbye world".to_string())]).into(),
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
    fn test_ls_and_pwd() {
        let line = "ls && pwd";
        let state = expr::State::new();
        let mut parser = Parser::new(state);
        let golden_set = Vec::from([AndOrNode::Andif(Box::new(AndIf {
            left: AndOrNode::Pipeline(Box::new(PipeLineExpr {
                pipeline: Vec::from([CompoundList::Commandexpr(CommandExpr {
                    command: Argument::Name("ls".to_string()),
                    arguments: Vec::new().into(),
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
                    arguments: Vec::new().into(),
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
                })])
                .into(),
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
                        arguments: Vec::new().into(),
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
                        arguments: Vec::from([Argument::Name("hello world".to_string())]).into(),
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
                })])
                .into(),
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
                })])
                .into(),
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
                })])
                .into(),
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
                        arguments: Vec::new().into(),
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
                        arguments: Vec::from([Argument::Name("hello world".to_string())]).into(),
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
                        arguments: Vec::from([Argument::Name("goodbye world".to_string())]).into(),
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
                ])
                .into(),
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
                        arguments: Vec::new().into(),
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
                        arguments: Vec::from([Argument::Name("1".to_string())]).into(),
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
                            arguments: Vec::new().into(),
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
                            arguments: Vec::from([Argument::Name("2".to_string())]).into(),
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
                            arguments: Vec::from([Argument::Name("3".to_string())]).into(),
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
                        arguments: Vec::new().into(),
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
                        arguments: Vec::from([Argument::Name("1".to_string())]).into(),
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
                            arguments: Vec::new().into(),
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
                            arguments: Vec::from([Argument::Name("2".to_string())]).into(),
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

    #[test]
    fn test_while_loop_do_line() {
        let line = "while true; do\necho 'hello world'\ndone";
        let state = expr::State::new();
        let mut parser = Parser::new(state);
        let golden_set = Vec::from([AndOrNode::Pipeline(Box::new(PipeLineExpr {
            pipeline: Vec::from([CompoundList::Whileexpr(WhileExpr {
                condition: AndOrNode::Pipeline(Box::new(PipeLineExpr {
                    pipeline: Vec::from([CompoundList::Commandexpr(CommandExpr {
                        command: Argument::Name("true".to_string()),
                        arguments: Vec::new().into(),
                        assignment: None,
                    })]),
                    capture_out: None,
                    file_redirect: None,
                    background: false,
                    state: expr::State::new(),
                })),
                body: Vec::from([PipeLineExpr {
                    pipeline: Vec::from([CompoundList::Commandexpr(CommandExpr {
                        command: Argument::Name("echo".to_string()),
                        arguments: Vec::from([Argument::Name("hello world".to_string())]).into(),
                        assignment: None,
                    })]),
                    capture_out: None,
                    file_redirect: None,
                    background: false,
                    state: expr::State::new(),
                }]),
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
    fn test_while_loop_do_new_line() {
        let line = "while true;\ndo\necho 'hello world'\ndone";
        let state = expr::State::new();
        let mut parser = Parser::new(state);
        let golden_set = Vec::from([AndOrNode::Pipeline(Box::new(PipeLineExpr {
            pipeline: Vec::from([CompoundList::Whileexpr(WhileExpr {
                condition: AndOrNode::Pipeline(Box::new(PipeLineExpr {
                    pipeline: Vec::from([CompoundList::Commandexpr(CommandExpr {
                        command: Argument::Name("true".to_string()),
                        arguments: Vec::new().into(),
                        assignment: None,
                    })]),
                    capture_out: None,
                    file_redirect: None,
                    background: false,
                    state: expr::State::new(),
                })),
                body: Vec::from([PipeLineExpr {
                    pipeline: Vec::from([CompoundList::Commandexpr(CommandExpr {
                        command: Argument::Name("echo".to_string()),
                        arguments: Vec::from([Argument::Name("hello world".to_string())]).into(),
                        assignment: None,
                    })]),
                    capture_out: None,
                    file_redirect: None,
                    background: false,
                    state: expr::State::new(),
                }]),
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
    fn test_semicolon_seperated() {
        let line = "echo 'hello world'; echo 'goodbye world'";
        let state = expr::State::new();
        let mut parser = Parser::new(state);
        let golden_set = Vec::from([
            AndOrNode::Pipeline(Box::new(PipeLineExpr {
                pipeline: Vec::from([CompoundList::Commandexpr(CommandExpr {
                    command: Argument::Name("echo".to_string()),
                    arguments: Vec::from([Argument::Name("hello world".to_string())]).into(),
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
                    arguments: Vec::from([Argument::Name("goodbye world".to_string())]).into(),
                    assignment: None,
                })]),
                capture_out: None,
                file_redirect: None,
                background: false,
                state: expr::State::new(),
            })),
        ]);
        parser.parse(&line);
        println!("{:#?}", parser.exprs);
        // println!("{:#?}", golden_set);
        assert!(parser.err.is_empty());
        if !parser.err.is_empty() {
            println!("Error: {}", parser.err);
        }
        for (i, expr) in golden_set.into_iter().enumerate() {
            assert!(parser.exprs[i].eq(&expr));
        }
    }

    #[test]
    fn test_parameter_expansion() {
        let line = "echo ${PWD}";
        let state = expr::State::new();
        let mut parser = Parser::new(state);
        let golden_set = Vec::from([AndOrNode::Pipeline(Box::new(PipeLineExpr {
            pipeline: Vec::from([CompoundList::Commandexpr(CommandExpr {
                command: Argument::Name("echo".to_string()),
                arguments: Vec::from([Argument::Expansion(ExpansionExpr::ParameterExpansion(
                    "PWD".to_string(),
                ))])
                .into(),
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
    fn test_var_and_expansion() {
        let line = "echo $X${PWD}";
        let state = expr::State::new();
        let mut parser = Parser::new(state);
        let golden_set = Vec::from([AndOrNode::Pipeline(Box::new(PipeLineExpr {
            pipeline: Vec::from([CompoundList::Commandexpr(CommandExpr {
                command: Argument::Name("echo".to_string()),
                arguments: Vec::from([Argument::Merge(MergeExpr {
                    left: Box::new(Argument::Variable(VariableLookup {
                        name: "X".to_string(),
                    })),
                    right: Box::new(Argument::Expansion(ExpansionExpr::ParameterExpansion(
                        "PWD".to_string(),
                    ))),
                })])
                .into(),
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
    fn test_var_and_expansion_with_subshell() {
        let line = "echo $X$(pwd)";
        let state = expr::State::new();
        let mut parser = Parser::new(state);
        let golden_set = Vec::from([AndOrNode::Pipeline(Box::new(PipeLineExpr {
            pipeline: Vec::from([CompoundList::Commandexpr(CommandExpr {
                command: Argument::Name("echo".to_string()),
                arguments: Vec::from([Argument::Merge(MergeExpr {
                    left: Box::new(Argument::Variable(VariableLookup {
                        name: "X".to_string(),
                    })),
                    right: Box::new(Argument::SubShell(SubShellExpr {
                        shell: "pwd".to_string(),
                    })),
                })])
                .into(),
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
    fn test_assignment_statement() {
        let line = "X=1 echo $X";
        let state = expr::State::new();
        let mut parser = Parser::new(state);
        let golden_set = Vec::from([AndOrNode::Pipeline(Box::new(PipeLineExpr {
            pipeline: Vec::from([CompoundList::Commandexpr(CommandExpr {
                command: Argument::Name("echo".to_string()),
                arguments: Vec::from([Argument::Variable(VariableLookup {
                    name: "X".to_string(),
                })])
                .into(),
                assignment: Some(AssignmentExpr {
                    key: "X".to_string(),
                    val: Argument::Name("1".to_string()),
                }),
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
    fn test_file_redirect() {
        let line = "ls > /tmp/file";
        let state = expr::State::new();
        let mut parser = Parser::new(state);
        let golden_set = Vec::from([AndOrNode::Pipeline(Box::new(PipeLineExpr {
            pipeline: Vec::from([CompoundList::Commandexpr(CommandExpr {
                command: Argument::Name("ls".to_string()),
                arguments: Vec::new().into(),
                assignment: None,
            })]),
            capture_out: None,
            file_redirect: Some(RedirectExpr {
                file: expr::Argument::Name("/tmp/file".to_string()),
                mode: expr::RedirectType::Out,
                file_descriptor: 1,
            }),
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
    fn test_file_append() {
        let line = "ls >> /tmp/file";
        let state = expr::State::new();
        let mut parser = Parser::new(state);
        let golden_set = Vec::from([AndOrNode::Pipeline(Box::new(PipeLineExpr {
            pipeline: Vec::from([CompoundList::Commandexpr(CommandExpr {
                command: Argument::Name("ls".to_string()),
                arguments: Vec::new().into(),
                assignment: None,
            })]),
            capture_out: None,
            file_redirect: Some(RedirectExpr {
                file: expr::Argument::Name("/tmp/file".to_string()),
                mode: expr::RedirectType::OutAppend,
                file_descriptor: 1,
            }),
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
}
