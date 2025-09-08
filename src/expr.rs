pub mod change_dir;
use crate::parser::Parser;
use shared_child::SharedChild;
use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::fmt::{Debug, Display, Formatter};
use std::fs::File;
use std::io::{PipeReader, PipeWriter, Read, Write};
use std::process;
use std::process::Stdio;
use std::process::{Command, ExitStatus};
use std::rc::Rc;
use std::sync::Arc;
use std::{env, io};

pub type FunctionStack = Rc<RefCell<Vec<Rc<Vec<Argument>>>>>; // ??

pub struct State {
    pub fg_jobs: Vec<Job>,
    pub bg_jobs: Vec<Job>,
    pub prev_status: i32,
    pub built_ins: HashMap<String, BuiltIn>,
    pub functions: HashMap<String, Rc<RefCell<Vec<PipeLineExpr>>>>,
    pub argstack: FunctionStack,
}

impl State {
    pub fn new() -> Rc<RefCell<State>> {
        #[allow(clippy::arc_with_non_send_sync)]
        Rc::new(RefCell::new(State {
            bg_jobs: Vec::new(),
            fg_jobs: Vec::new(),
            prev_status: 0,
            argstack: FunctionStack::new(RefCell::new(Vec::new())),
            functions: HashMap::new(),
            built_ins: HashMap::from([
                (
                    "cd".to_string(),
                    BuiltIn {
                        name: "cd".to_string(),
                        command: Rc::new(
                            |args: &Vec<Argument>,
                             state: Rc<RefCell<State>>,
                             _stdin: &Option<PipeReader>,
                             _stdout: &Option<PipeWriter>|
                             -> i32 {
                                change_dir::ChangeDir::new(&args[0].eval(&state)).eval()
                            },
                        ),
                    },
                ),
                (
                    "jobs".to_string(),
                    BuiltIn {
                        name: "jobs".to_string(),
                        command: Rc::new(
                            |args: &Vec<Argument>,
                             state: Rc<RefCell<State>>,
                             _stdin: &Option<PipeReader>,
                             _stdout: &Option<PipeWriter>|
                             -> i32 {
                                let opt = args.first().and_then(|arg| match arg {
                                    Argument::Name(arg) => Some(arg.as_str()),
                                    _ => None,
                                });
                                handle_jobs_cmd(opt, &state);
                                0
                            },
                        ),
                    },
                ),
                (
                    "true".to_string(),
                    BuiltIn {
                        name: "true".to_string(),
                        command: Rc::new(
                            |_: &Vec<Argument>,
                             _: Rc<RefCell<State>>,
                             _: &Option<PipeReader>,
                             _: &Option<PipeWriter>|
                             -> i32 { 0 },
                        ),
                    },
                ),
                (
                    "false".to_string(),
                    BuiltIn {
                        name: "true".to_string(),
                        command: Rc::new(
                            |_: &Vec<Argument>,
                             _: Rc<RefCell<State>>,
                             _: &Option<PipeReader>,
                             _: &Option<PipeWriter>|
                             -> i32 { 1 },
                        ),
                    },
                ),
                (
                    "astview".to_string(),
                    BuiltIn {
                        name: "astview".to_string(),
                        command: Rc::new(
                            |args: &Vec<Argument>,
                             state: Rc<RefCell<State>>,
                             _: &Option<PipeReader>,
                             _: &Option<PipeWriter>|
                             -> i32 {
                                let mut parser = Parser::new(state.clone());
                                parser.parse(&args[0].eval(&state));
                                println!("{:#?}", parser.exprs);
                                0
                            },
                        ),
                    },
                ),
                (
                    "help".to_string(),
                    BuiltIn {
                        name: "help".to_string(),
                        command: Rc::new(
                            |_: &Vec<Argument>,
                             _: Rc<RefCell<State>>,
                             _: &Option<PipeReader>,
                             _: &Option<PipeWriter>|
                             -> i32 {
                                println!("slush: A shell you can drink!");
                                println!("\nBuiltins:");
                                println!("  cd <dir> - change directory");
                                println!("  exit [code] - exit the shell, optionally with a code");
                                println!(
                                    "  astview '<command>' - view the abstract syntax tree of a command"
                                );
                                println!("  true - return 0");
                                println!("  false - return 1");
                                println!("  help - print this message");
                                0
                            },
                        ),
                    },
                ),
                (
                    "exit".to_string(),
                    BuiltIn {
                        name: "exit".to_string(),
                        command: Rc::new(
                            |args: &Vec<Argument>,
                             state: Rc<RefCell<State>>,
                             _: &Option<PipeReader>,
                             _: &Option<PipeWriter>|
                             -> i32 {
                                if !args.is_empty() {
                                    std::process::exit(
                                        args[0].eval(&state).parse().unwrap_or_default(),
                                    );
                                } else {
                                    std::process::exit(0);
                                }
                            },
                        ),
                    },
                ),
            ]),
        }))
    }
}

// sort of a hack to always assume all states are the same ? seems JANK
impl PartialEq for State {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl Debug for State {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("State").finish()
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct FunctionExpr {
    pub fname: String,
    pub commands: Vec<PipeLineExpr>,
    pub args: FunctionStack,
}

impl FunctionExpr {
    pub fn eval(&mut self, state: &Rc<RefCell<State>>) -> i32 {
        state.borrow_mut().functions.insert(
            self.fname.clone(),
            Rc::new(RefCell::new(self.commands.clone())),
        );
        0
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct VariableLookup {
    pub name: String,
}

#[derive(Debug, PartialEq, Clone)]
pub enum RedirectType {
    Out,
    OutAppend,
    In,
}

#[derive(Debug, PartialEq, Clone)]
pub struct RedirectExpr {
    pub file: Argument,
    pub mode: RedirectType,
    pub file_descriptor: u32,
}

// How do we made these outputs streams? it would be nice to have it feed
// between two child CommandExprs as they are creating them...
#[derive(Debug, PartialEq, Clone)]
pub struct CommandExpr {
    pub command: Argument,
    pub arguments: Rc<Vec<Argument>>,
    pub assignment: Option<AssignmentExpr>,
}

#[derive(Debug, Clone)]
pub struct PipeLineExpr {
    pub pipeline: Vec<CompoundList>,
    pub capture_out: Option<Rc<RefCell<String>>>,
    pub file_redirect: Option<RedirectExpr>,
    pub background: bool,
    pub state: Rc<RefCell<State>>,
}

impl PartialEq for PipeLineExpr {
    fn eq(&self, other: &Self) -> bool {
        self.pipeline == other.pipeline
            && self.capture_out == other.capture_out
            && self.file_redirect == other.file_redirect
            && self.background == other.background
    }
}

#[derive(Debug, PartialEq, Clone)]
pub enum IfBranch {
    Elif(Box<IfExpr>),
    Else(Vec<PipeLineExpr>),
}

// instead of making this a tree could i make it a vector?
#[derive(Debug, PartialEq, Clone)]
pub struct IfExpr {
    pub condition: PipeLineExpr,
    pub if_branch: Vec<PipeLineExpr>,
    pub else_branch: Option<IfBranch>,
}

impl IfExpr {
    pub fn eval(&mut self) -> Result<i32, String> {
        if self.condition.eval()? == 0 {
            for command in &mut self.if_branch {
                command.eval()?;
            }
        } else if let Some(branch) = &mut self.else_branch {
            match branch {
                IfBranch::Elif(ifb) => {
                    ifb.eval()?;
                }
                IfBranch::Else(elseb) => {
                    for command in elseb {
                        command.eval()?;
                    }
                }
            };
        }
        Ok(0)
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct WhileExpr {
    pub condition: AndOrNode,
    pub body: Vec<PipeLineExpr>,
}

impl WhileExpr {
    pub fn eval(&mut self) -> Result<i32, String> {
        while self.condition.eval()? == 0 {
            for command in &mut self.body {
                command.eval()?;
            }
        }
        Ok(0)
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct ForExpr {
    pub name: String,
    pub list: Vec<Argument>,
    pub commands: Vec<PipeLineExpr>,
}

impl ForExpr {
    pub fn eval(&mut self, state: &Rc<RefCell<State>>) -> Result<i32, String> {
        let mut ret = 0;
        for arg in &self.list {
            unsafe {
                env::set_var(&self.name, arg.eval(state));
            }
            for command in &mut self.commands {
                ret = command.eval()?;
            }
        }
        Ok(ret)
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct NotExpr {
    pub condition: AndOrNode,
}

impl NotExpr {
    pub fn eval(&mut self) -> Result<i32, String> {
        if self.condition.eval()? == 0 {
            Ok(1)
        } else {
            Ok(0)
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
pub enum CompoundList {
    Ifexpr(IfExpr),
    Whileexpr(WhileExpr),
    Forexpr(ForExpr),
    Commandexpr(CommandExpr),
    Functionexpr(FunctionExpr),
}

#[derive(Debug, PartialEq, Clone)]
pub enum AndOrNode {
    Pipeline(Box<PipeLineExpr>),
    Andif(Box<AndIf>),
    Orif(Box<OrIf>),
    Notif(Box<NotExpr>),
}

impl AndOrNode {
    pub fn eval(&mut self) -> Result<i32, String> {
        match self {
            AndOrNode::Pipeline(pl) => pl.eval(),
            AndOrNode::Andif(and) => and.eval(),
            AndOrNode::Orif(or) => or.eval(),
            AndOrNode::Notif(not) => not.eval(),
        }
    }

    pub fn set_output_capture(&mut self, capture: Rc<RefCell<String>>) {
        match self {
            AndOrNode::Pipeline(pl) => pl.set_output_capture(capture),
            AndOrNode::Andif(and) => and.set_output_capture(capture),
            AndOrNode::Orif(or) => or.set_output_capture(capture),
            AndOrNode::Notif(not) => not.condition.set_output_capture(capture),
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct OrIf {
    pub left: AndOrNode,
    pub right: AndOrNode,
}

impl OrIf {
    fn eval(&mut self) -> Result<i32, String> {
        let ll = self.left.eval()?;
        if ll != 0 {
            return self.right.eval();
        }
        Ok(ll)
    }

    pub fn set_output_capture(&mut self, capture: Rc<RefCell<String>>) {
        self.left.set_output_capture(capture.clone());
        self.right.set_output_capture(capture.clone());
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct AndIf {
    pub left: AndOrNode,
    pub right: AndOrNode,
}

impl AndIf {
    fn eval(&mut self) -> Result<i32, String> {
        let ll = self.left.eval()?;
        let rr = self.right.eval()?;
        if ll != 0 {
            return Ok(ll);
        }
        Ok(rr)
    }

    pub fn set_output_capture(&mut self, capture: Rc<RefCell<String>>) {
        self.left.set_output_capture(capture.clone()); // Line 123 !
        self.right.set_output_capture(capture.clone());
    }
}

// pub struct And IF
#[derive(Debug, PartialEq, Clone)]
pub struct AssignmentExpr {
    pub key: String,
    pub val: Argument,
}

// Subshell is simply a wrapper around a string which can be fed into a
// parser, evaluated and stdout returned.
#[derive(Debug, PartialEq, Clone)]
pub struct SubShellExpr {
    pub shell: String,
}

impl SubShellExpr {
    pub fn stdout(&self) -> String {
        let mut parser = Parser::new(State::new());
        let shell_output: Rc<RefCell<String>> = Default::default();
        parser.parse(&self.shell);
        for mut expr in parser.exprs {
            expr.set_output_capture(shell_output.clone());
            let _ = expr.eval();
        }
        shell_output.borrow().clone()
    }
}

impl AssignmentExpr {
    fn eval(&mut self, state: &Rc<RefCell<State>>) -> i32 {
        unsafe {
            env::set_var(&self.key, self.val.eval(state));
        }
        0
    }
}

impl CommandExpr {
    pub fn build_command_str(&self, state: &Rc<RefCell<State>>) -> CommandStr {
        let com = self.command.eval(state);
        let mut parts = vec![com];
        for arg in &*self.arguments {
            parts.push(arg.eval(state));
        }
        CommandStr { parts }
    }
}

#[derive(Debug, Clone)]
pub struct CommandStr {
    parts: Vec<String>,
}

impl CommandStr {
    pub fn build_command(&self) -> Box<Command> {
        let mut cmd = Box::new(Command::new(&self.parts[0]));
        for arg in &self.parts[1..] {
            cmd.arg(arg);
        }
        cmd
    }
}

type BuiltInEval =
    Rc<dyn Fn(&Vec<Argument>, Rc<RefCell<State>>, &Option<PipeReader>, &Option<PipeWriter>) -> i32>;
pub struct BuiltIn {
    name: String,
    command: BuiltInEval,
}

impl Clone for BuiltIn {
    fn clone(&self) -> BuiltIn {
        BuiltIn {
            name: self.name.clone(),
            command: self.command.clone(),
        }
    }
}

impl Debug for BuiltIn {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("BuiltIn").field("name", &self.name).finish()
    }
}

#[derive(Debug, Clone)]
enum SlushJobType {
    Builtin(BuiltIn, Rc<Vec<Argument>>),
    Function(String, Rc<Vec<Argument>>),
    Child(Arc<SharedChild>),
}

struct SlushJob {
    jobtype: SlushJobType,
    stdin: Option<PipeReader>,
    stdout: Option<PipeWriter>, // TODO: fucking figure out a lifetime
}

impl SlushJob {
    fn new(
        jobtype: SlushJobType,
        stdin: Option<PipeReader>,
        stdout: Option<PipeWriter>,
    ) -> SlushJob {
        Self {
            jobtype,
            stdin,
            stdout,
        }
    }
}

impl PipeLineExpr {
    fn assemble_pipeline(&mut self) -> Result<Vec<SlushJob>, String> {
        let mut jobs: Vec<SlushJob> = Vec::new();
        let sz = self.pipeline.len();
        for (i, expr) in self.pipeline.iter_mut().enumerate() {
            match expr {
                CompoundList::Ifexpr(ifxpr) => ifxpr.eval()?,
                CompoundList::Whileexpr(whlexpr) => whlexpr.eval()?,
                CompoundList::Forexpr(forexpr) => forexpr.eval(&self.state.clone())?,
                CompoundList::Functionexpr(func) => func.eval(&self.state.clone()),
                CompoundList::Commandexpr(exp) => {
                    if let Some(ref mut ass) = exp.assignment {
                        ass.eval(&self.state.clone());
                    }

                    if let Argument::Name(arg) = &exp.command
                        && arg.is_empty()
                    {
                        continue;
                    }

                    let base_command = exp.command.eval(&self.state.clone());

                    if let Some(command) = self.state.borrow().built_ins.get(&base_command) {
                        jobs.push(SlushJob::new(
                            SlushJobType::Builtin(command.clone(), exp.arguments.clone()),
                            None,
                            None,
                        ));
                        continue;
                    }

                    if self.state.borrow().functions.contains_key(&base_command) {
                        jobs.push(SlushJob::new(
                            SlushJobType::Function(base_command, exp.arguments.clone()),
                            None,
                            None,
                        ));
                        continue;
                    }

                    let cmd_str = exp.build_command_str(&self.state.clone());
                    let mut cmd = cmd_str.build_command();

                    let mut state = self.state.borrow_mut();

                    if let Some(job) = jobs.last_mut() {
                        match &job.jobtype {
                            SlushJobType::Function(_, _) => None,
                            SlushJobType::Builtin(_, _) => {
                                if let Some(stdout) = &job.stdout {
                                    cmd.stdin(stdout.try_clone().expect("Couldn't clone pipe"));
                                }
                                Some(())
                            }
                            SlushJobType::Child(pchild) => {
                                {
                                    cmd.stdin(pchild.take_stdout().unwrap());
                                }
                                Some(())
                            }
                        };
                    };
                    if let Some(file_redirect) = &self.file_redirect
                        && file_redirect.mode == RedirectType::In
                    {
                        cmd.stdin(Stdio::piped());
                    }

                    if i < sz - 1
                        || self.capture_out.is_some()
                        || (self.file_redirect.as_ref().is_some()
                            && self.file_redirect.as_ref().unwrap().mode != RedirectType::In)
                    {
                        cmd.stdout(Stdio::piped());
                    }

                    jobs.push(match cmd.spawn() {
                        Ok(c) => match SharedChild::new(c) {
                            Ok(sc) => SlushJob::new(SlushJobType::Child(Arc::new(sc)), None, None),
                            Err(v) => {
                                return Err(format!(
                                    "Error creating shared child {}: {}",
                                    exp.command.eval(&self.state),
                                    v
                                ));
                            }
                        },
                        Err(v) => {
                            return Err(format!(
                                "Error spawning {}: {}",
                                exp.command.eval(&self.state),
                                v
                            ));
                        }
                    });

                    if let SlushJobType::Child(child) = &jobs.last_mut().as_ref().unwrap().jobtype {
                        let job = Job {
                            pid: child.id(),
                            child: child.clone(),
                            cmd: cmd_str,
                        };

                        if self.background {
                            state.bg_jobs.push(job);
                        } else {
                            state.fg_jobs.push(job);
                        }
                    }
                    0
                }
            };
        }
        Ok(jobs)
    }

    fn eval(&mut self) -> Result<i32, String> {
        let jobs = self.assemble_pipeline()?;

        let mut prev_child: Option<Arc<SharedChild>> = None;
        if let Some(job) = jobs.last()
            && let SlushJobType::Child(child) = &job.jobtype
        {
            prev_child = Some(child.clone());
        }
        let mut exit_status: i32 = 0;
        if let Some(rcstr) = &self.capture_out {
            if !self.background {
                let outie = wait_with_output(&prev_child.unwrap());
                rcstr
                    .borrow_mut()
                    .push_str(&String::from_utf8(outie.stdout.clone()).unwrap());
                if rcstr.borrow().ends_with('\n') {
                    rcstr.borrow_mut().pop();
                }
                exit_status = outie.status.expect("Couldn't get exit code from prev job");
            } else {
                exit_status = 0;
            }
        } else if self.file_redirect.is_some() {
            let file_redirect = self.file_redirect.as_ref().unwrap();

            let filename = file_redirect.file.eval(&self.state);
            let mode = &file_redirect.mode;
            // will these error silently?
            let mut file = if *mode == RedirectType::OutAppend {
                match File::options().append(true).open(filename) {
                    Ok(f) => f,
                    Err(_) => return Ok(1),
                }
            } else if *mode == RedirectType::Out {
                match File::create(filename) {
                    Ok(f) => f,
                    Err(_) => return Ok(1),
                }
            } else if *mode == RedirectType::In {
                match File::open(filename) {
                    Ok(f) => f,
                    Err(_) => return Ok(1),
                }
            } else {
                panic!("Unexpected for redirect! Error Error!");
            };
            if *mode == RedirectType::In {
                let mut buffer: Box<[u8]> = Box::new([0; 4096]);
                let mut stdin = match prev_child.clone().unwrap().take_stdin() {
                    Some(s) => s,
                    None => {
                        panic!("couldn't acquire stdin of process...");
                    }
                };

                loop {
                    let n = match file.read(&mut buffer) {
                        Ok(n) => n,
                        Err(e) => {
                            println!("{e:?}");
                            return Ok(1);
                        }
                    };
                    if n == 0 {
                        break;
                    }
                    if let Err(e) = stdin.write(&buffer[..n]) {
                        panic!("Error writing to stdin: {e}");
                    }
                }
                drop(stdin);
                let status = prev_child.unwrap().wait().unwrap();
                exit_status = status.code().unwrap_or(130);
            } else {
                let outie = wait_with_output(&prev_child.unwrap());
                let _ = file.write_all(&outie.stdout.clone());
            }
        } else if let Some(job) = jobs.last() {
            match &job.jobtype {
                SlushJobType::Child(child) => {
                    if !self.background {
                        let status = child.wait().unwrap();
                        exit_status = status.code().unwrap_or(130);
                    } else {
                        exit_status = 0;
                    }
                }
                SlushJobType::Builtin(builtin, args) => {
                    exit_status =
                        (builtin.command)(args, self.state.clone(), &job.stdin, &job.stdout);
                }
                SlushJobType::Function(function, args) => {
                    let argstack = self.state.borrow().argstack.clone();
                    let aa = args
                        .iter()
                        .map(|a| -> Argument { Argument::Name(a.eval(&self.state)) })
                        .collect();
                    argstack.borrow_mut().push(Rc::new(aa));
                    let mut functions = self.state.borrow_mut().functions.clone();
                    let pl = functions.get_mut(function).unwrap().clone();
                    {
                        let mut ppl = pl.borrow_mut();
                        for pipeline in ppl.iter_mut() {
                            exit_status = pipeline.eval()?;
                        }
                    }

                    let argstack = &mut self.state.borrow_mut().argstack;
                    argstack.borrow_mut().pop();
                }
            }
        }

        Ok(exit_status)
    }
}

impl PipeLineExpr {
    pub fn set_output_capture(&mut self, capture: Rc<RefCell<String>>) {
        self.capture_out = Some(capture);
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct MergeExpr {
    pub left: Box<Argument>,
    pub right: Box<Argument>,
}

impl MergeExpr {
    pub fn eval(&self, state: &Rc<RefCell<State>>) -> String {
        self.left.eval(state) + &self.right.eval(state)
    }
}

#[derive(Debug, PartialEq, Clone)]
pub enum ExpansionExpr {
    ParameterExpansion(String), // the same as Argument::Variable
    StringLengthExpansion(String),
    ParameterSubstitute(String, String), // if null or unset sets to default
    ParameterAssign(String, String),     // if null or unset sets to default
    ParameterError(String, String),      // if null sets null
}

impl ExpansionExpr {
    fn eval(&self, state: &Rc<RefCell<State>>) -> String {
        match self {
            ExpansionExpr::ParameterExpansion(var) => {
                get_variable(var.clone(), state).unwrap_or_default()
            }
            ExpansionExpr::StringLengthExpansion(var) => get_variable(var.clone(), state)
                .unwrap_or_default()
                .len()
                .to_string(),
            ExpansionExpr::ParameterSubstitute(var, default) => {
                if let Some(v) = get_variable(var.clone(), state) {
                    v
                } else {
                    default.clone()
                }
            }
            ExpansionExpr::ParameterError(var, err) => {
                eprintln!("slush: {var}: {err}");
                std::process::exit(1);
            }
            ExpansionExpr::ParameterAssign(var, default) => {
                if let Some(v) = get_variable(var.clone(), state) {
                    v
                } else {
                    unsafe {
                        env::set_var(var, default);
                    }
                    default.clone()
                }
            }
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
pub enum Argument {
    Name(String),
    QuoteString(String),
    Variable(VariableLookup),
    SubShell(SubShellExpr),
    Merge(MergeExpr),
    Expansion(ExpansionExpr),
}

impl Argument {
    fn eval(&self, state: &Rc<RefCell<State>>) -> String {
        match self {
            Argument::Name(n) => n.clone(),
            Argument::Variable(variable) => {
                get_variable(variable.name.clone(), state).unwrap_or_default()
            }
            Argument::SubShell(ss) => ss.stdout(),
            Argument::Merge(merge) => merge.eval(state),
            Argument::Expansion(expansion) => expansion.eval(state),
            Argument::QuoteString(string) => evaluate_string(string, state).unwrap_or_default(),
        }
    }
}

fn evaluate_string(string: &str, state: &Rc<RefCell<State>>) -> Option<String> {
    let mut parser = Parser::new(state.clone());
    let ret = match parser.parse_double_quoted_string(string) {
        Ok(args) => args
            .into_iter()
            .map(|a| a.eval(state))
            .reduce(|whole, next| whole + &next)
            .unwrap(),
        Err(e) => {
            println!("Slush Error {e}");
            String::default()
        }
    };
    Some(ret)
}

fn get_variable(var: String, state: &Rc<RefCell<State>>) -> Option<String> {
    let s = state.borrow();
    match var.as_str() {
        "0" => Some(String::from("slush")),
        "!" => Some(if let Some(job) = s.bg_jobs.last() {
            job.child.id().to_string()
        } else {
            String::from("0")
        }),
        "?" => Some(state.borrow().prev_status.to_string()),
        "$" => Some(process::id().to_string()),
        "@" | "*" => {
            // I don't think '*' is the 'spec complient'
            let argstack = s.argstack.borrow();
            if argstack.is_empty() {
                return Some(String::default());
            }
            Some(
                argstack
                    .last()
                    .unwrap()
                    .iter()
                    .map(|arg| arg.eval(state))
                    .reduce(|whole, new| whole.to_owned() + " " + &new)
                    .unwrap()
                    .to_string(),
            )
        }
        "#" => {
            let argstack = s.argstack.borrow();
            if argstack.is_empty() {
                return Some(String::default());
            }
            Some(format!("{}", argstack.last().unwrap().len()))
        }
        "-" => {
            panic!("'{var}' parameters are not yet supported")
        }
        _ => {
            if let Ok(number) = var.parse::<usize>() {
                let argstack = s.argstack.clone();

                let args = argstack.borrow();
                args.last().map(|a| {
                    a.get(number - 1)
                        .unwrap_or(&Argument::Name("".to_string()))
                        .eval(state)
                })
            } else {
                let text = env::var(var).unwrap_or_default();
                if text.is_empty() { None } else { Some(text) }
            }
        }
    }
}

fn wait_with_output(child: &SharedChild) -> Output {
    drop(child.take_stdin());
    let cid = child.id();

    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    match (child.take_stdout(), child.take_stderr()) {
        (None, None) => {}
        (Some(mut out), None) => {
            out.read_to_end(&mut stdout)
                .unwrap_or_else(|_| panic!("Error reading stdout from pid {cid}"));
        }
        (None, Some(mut err)) => {
            err.read_to_end(&mut stderr)
                .unwrap_or_else(|_| panic!("Error reading from stderr from pid {cid}"));
        }
        (Some(mut out), Some(mut err)) => {
            let out_handle = std::thread::spawn(move || {
                out.read_to_end(&mut stdout)
                    .unwrap_or_else(|_| panic!("Error reading stdout from pid {cid}"));
                stdout
            });
            let err_handle = std::thread::spawn(move || {
                err.read_to_end(&mut stderr)
                    .unwrap_or_else(|_| panic!("Error reading from stderr from pid {cid}"));
                stderr
            });

            stdout = out_handle.join().expect("thread panicked");
            stderr = err_handle.join().expect("thread panicked");
        }
    }

    let status = child
        .wait()
        .unwrap_or_else(|_| panic!("Error waiting from pid {cid}"));
    Output {
        status: status.code(),
        stdout,
        _stderr: stderr,
    }
}

#[derive(Clone)]
pub struct Output {
    status: Option<i32>,
    stdout: Vec<u8>,
    _stderr: Vec<u8>,
}

fn handle_jobs_cmd(opt: Option<&str>, state: &Rc<RefCell<State>>) {
    match opt {
        None => {
            let state = state.borrow();
            for (job_num, job) in state.bg_jobs.iter().enumerate() {
                // Todo: display <current>.
                print!("[{}] {}", job_num + 1, job.status());
                for part in &job.cmd.parts {
                    print!(" {part}");
                }
                println!();
            }
        }
        Some("-p") => {
            let state = state.borrow();
            for job in state.bg_jobs.iter() {
                println!("{}", job.pid);
            }
        }
        Some("-l") => {
            println!("The option `-l` is not supported at the moment");
        }
        Some(opt) => {
            println!("invalid option `{opt}`");
        }
    }
}

#[derive(Debug)]
enum Status {
    Running,
    Done(ExitStatus),
    Unknown(io::Error),
}

impl Display for Status {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Status::Running => write!(f, "Running"),
            Status::Done(code) => write!(f, "Done({code})"),
            Status::Unknown(e) => write!(f, "Unknown({e})"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Job {
    pub child: Arc<SharedChild>,
    pub cmd: CommandStr,
    pub pid: u32,
}

impl Job {
    fn status(&self) -> Status {
        match self.child.try_wait() {
            Ok(Some(status)) => Status::Done(status),
            Ok(None) => Status::Running,
            Err(e) => Status::Unknown(e),
        }
    }
}
