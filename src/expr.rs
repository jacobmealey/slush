pub mod change_dir;
use crate::parser::Parser;
use shared_child::SharedChild;
use std::cell::RefCell;
use std::env;
use std::fs::File;
use std::io::{Read, Write};
use std::process;
use std::process::Command;
use std::process::Stdio;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

#[derive(Debug)]
pub struct State {
    pub fg_jobs: Vec<Job>,
    pub bg_jobs: Vec<Job>,
    pub prev_status: i32,
}

impl State {
    pub fn new() -> Arc<Mutex<State>> {
        Arc::new(Mutex::new(State {
            bg_jobs: Vec::new(),
            fg_jobs: Vec::new(),
            prev_status: 0,
        }))
    }
}

// sort of a hack to always assume all states are the same ? seems JANK
impl PartialEq for State {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

#[derive(Debug, PartialEq)]
pub struct VariableLookup {
    pub name: String,
}

// How do we made these outputs streams? it would be nice to have it feed
// between two child CommandExprs as they are creating them...
#[derive(Debug, PartialEq)]
pub struct CommandExpr {
    pub command: Argument,
    pub arguments: Vec<Argument>,
    pub assignment: Option<AssignmentExpr>,
}

#[derive(Debug)]
pub struct PipeLineExpr {
    pub pipeline: Vec<CompoundList>,
    pub capture_out: Option<Rc<RefCell<String>>>,
    pub file_redirect: Option<Argument>,
    pub background: bool,
    pub state: Arc<Mutex<State>>,
}

impl PartialEq for PipeLineExpr {
    fn eq(&self, other: &Self) -> bool {
        self.pipeline == other.pipeline
            && self.capture_out == other.capture_out
            && self.file_redirect == other.file_redirect
            && self.background == other.background
    }
}

#[derive(Debug, PartialEq)]
pub enum IfBranch {
    Elif(Box<IfExpr>),
    Else(Vec<PipeLineExpr>),
}

// instead of making this a tree could i make it a vector?
#[derive(Debug, PartialEq)]
pub struct IfExpr {
    pub condition: PipeLineExpr,
    pub if_branch: Vec<PipeLineExpr>,
    pub else_branch: Option<IfBranch>,
    //pub elif_branch: Option<Box<IfExpr>>,
    //pub else_branch: Option<Vec<PipeLineExpr>>,
}

impl IfExpr {
    pub fn eval(&mut self) -> i32 {
        if self.condition.eval() == 0 {
            for command in &mut self.if_branch {
                command.eval();
            }
        } else if let Some(branch) = &mut self.else_branch {
            match branch {
                IfBranch::Elif(ifb) => {
                    ifb.eval();
                }
                IfBranch::Else(elseb) => {
                    for command in elseb {
                        command.eval();
                    }
                }
            };
        }
        0
    }
}

#[derive(Debug, PartialEq)]
pub enum CompoundList {
    Ifexpr(IfExpr),
    Commandexpr(CommandExpr),
}

#[derive(Debug, PartialEq)]
pub enum AndOrNode {
    Pipeline(Box<PipeLineExpr>),
    Andif(Box<AndIf>),
    Orif(Box<OrIf>),
}

impl AndOrNode {
    pub fn eval(&mut self) -> i32 {
        match self {
            AndOrNode::Pipeline(pl) => pl.eval(),
            AndOrNode::Andif(and) => and.eval(),
            AndOrNode::Orif(or) => or.eval(),
        }
    }

    pub fn set_output_capture(&mut self, capture: Rc<RefCell<String>>) {
        match self {
            AndOrNode::Pipeline(pl) => pl.set_output_capture(capture),
            AndOrNode::Andif(and) => and.set_output_capture(capture),
            AndOrNode::Orif(or) => or.set_output_capture(capture),
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct OrIf {
    pub left: AndOrNode,
    pub right: AndOrNode,
}

impl OrIf {
    fn eval(&mut self) -> i32 {
        let ll = self.left.eval();
        if ll != 0 {
            return self.right.eval();
        }
        ll
    }

    pub fn set_output_capture(&mut self, capture: Rc<RefCell<String>>) {
        self.left.set_output_capture(capture.clone());
        self.right.set_output_capture(capture.clone());
    }
}

#[derive(Debug, PartialEq)]
pub struct AndIf {
    pub left: AndOrNode,
    pub right: AndOrNode,
}

impl AndIf {
    fn eval(&mut self) -> i32 {
        let ll = self.left.eval();
        let rr = self.right.eval();
        if ll != 0 {
            return ll;
        }
        rr
    }

    pub fn set_output_capture(&mut self, capture: Rc<RefCell<String>>) {
        self.left.set_output_capture(capture.clone()); // Line 123 !
        self.right.set_output_capture(capture.clone());
    }
}

// pub struct And IF
#[derive(Debug, PartialEq)]
pub struct AssignmentExpr {
    pub key: String,
    pub val: Argument,
}

// Subshell is simply a wrapper around a string which can be fed into a
// parser, evaluated and stdout returned.
#[derive(Debug, PartialEq)]
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
            expr.eval();
        }
        let x = shell_output.borrow().clone();
        x
    }
}

impl AssignmentExpr {
    fn eval(&mut self, state: &Arc<Mutex<State>>) -> i32 {
        unsafe {
            env::set_var(&self.key, self.val.eval(state));
        }
        0
    }
}

impl CommandExpr {
    pub fn build_command_str(&self, state: &Arc<Mutex<State>>) -> CommandStr {
        let com = self.command.eval(state);
        let mut parts = vec![com];
        for arg in &self.arguments {
            parts.push(arg.eval(state));
        }
        CommandStr { parts }
    }
}

#[derive(Debug)]
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

impl PipeLineExpr {
    fn eval(&mut self) -> i32 {
        let sz = self.pipeline.len();
        let mut prev_child: Option<Arc<SharedChild>> = None;
        for (i, expr) in self.pipeline.iter_mut().enumerate() {
            match expr {
                CompoundList::Ifexpr(ifxpr) => ifxpr.eval(),
                CompoundList::Commandexpr(exp) => {
                    if let Some(ref mut ass) = exp.assignment {
                        ass.eval(&self.state.clone());
                    }

                    if let Argument::Name(arg) = &exp.command {
                        if arg.is_empty() {
                            continue;
                        }
                    }

                    let base_command = exp.command.eval(&self.state.clone());
                    // should built ins be there own special node on the tree?
                    if base_command == "cd" {
                        return change_dir::ChangeDir::new(&exp.arguments[0].eval(&self.state))
                            .eval();
                    } else if base_command == "jobs" {
                        let opt = exp.arguments.get(0).and_then(|arg| match arg {
                            Argument::Name(arg) => Some(arg.as_str()),
                            _ => None,
                        });
                        handle_jobs_cmd(opt, &self.state);
                    } else if base_command == "true" {
                        return 0;
                    } else if base_command == "false" {
                        return 1;
                    } else if base_command == "astview" {
                        let mut parser = Parser::new(self.state.clone());
                        parser.parse(&exp.arguments[0].eval(&self.state));
                        println!("{:#?}", parser.exprs);
                        return 0;
                    } else if base_command == "exit" {
                        if !exp.arguments.is_empty() {
                            std::process::exit(
                                exp.arguments[0]
                                    .eval(&self.state)
                                    .parse()
                                    .unwrap_or_default(),
                            );
                        } else {
                            std::process::exit(0);
                        }
                    } else if base_command == "help" {
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
                        return 0;
                    }

                    let mut cmd_str = exp.build_command_str(&self.state.clone());
                    let mut cmd = cmd_str.build_command();

                    let mut state = self.state.lock().expect("unable to acquire lock");

                    if let Some(pchild) = prev_child {
                        cmd.stdin(pchild.take_stdout().unwrap());
                    }
                    if i < sz - 1 || self.capture_out.is_some() || self.file_redirect.is_some() {
                        cmd.stdout(Stdio::piped());
                    }

                    prev_child = Some(match cmd.spawn() {
                        Ok(c) => match SharedChild::new(c) {
                            Ok(sc) => Arc::new(sc),
                            Err(v) => {
                                println!(
                                    "Error creating shared child {}: {}",
                                    exp.command.eval(&self.state),
                                    v
                                );
                                return 2;
                            }
                        },
                        Err(v) => {
                            println!("Error spawning {}: {}", exp.command.eval(&self.state), v);
                            return 2;
                        }
                    });

                    let child = prev_child.as_ref().unwrap().clone();
                    let job = Job {
                        pid: child.id(),
                        child,
                        cmd: cmd_str,
                    };

                    if self.background {
                        state.bg_jobs.push(job);
                    } else {
                        state.fg_jobs.push(job);
                    }
                    0
                }
            };
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
                println!("Spawning command in the background!");
                exit_status = 0;
            }
        } else if self.file_redirect.is_some() {
            let filename = self.file_redirect.as_ref().unwrap().eval(&self.state);
            let mut file = match File::create(filename) {
                Ok(f) => f,
                Err(_) => return 1,
            };
            let outie = wait_with_output(&prev_child.unwrap());
            let _ = file.write_all(&outie.stdout.clone());
        } else if prev_child.is_some() {
            if !self.background {
                let status = prev_child.unwrap().wait().unwrap();
                exit_status = status.code().unwrap_or(130);
            } else {
                exit_status = 0;
            }
        }

        exit_status
    }
}

impl PipeLineExpr {
    pub fn set_output_capture(&mut self, capture: Rc<RefCell<String>>) {
        self.capture_out = Some(capture);
    }
}

#[derive(Debug, PartialEq)]
pub struct MergeExpr {
    pub left: Box<Argument>,
    pub right: Box<Argument>,
}

impl MergeExpr {
    pub fn eval(&self, state: &Arc<Mutex<State>>) -> String {
        self.left.eval(state) + &self.right.eval(state)
    }
}

#[derive(Debug, PartialEq)]
pub enum ExpansionExpr {
    ParameterExpansion(String), // the same as Argument::Variable
    StringLengthExpansion(String),
    ParameterSubstitute(String, String), // if null or unset sets to default
    ParameterAssign(String, String),     // if null or unset sets to default
    ParameterError(String, String),      // if null sets null
}

impl ExpansionExpr {
    fn eval(&self, state: &Arc<Mutex<State>>) -> String {
        match self {
            ExpansionExpr::ParameterExpansion(var) => get_variable(var.clone(), state),
            ExpansionExpr::StringLengthExpansion(var) => {
                get_variable(var.clone(), state).len().to_string()
            }
            ExpansionExpr::ParameterSubstitute(var, default) => {
                if !get_variable(var.clone(), state).to_string().is_empty() {
                    get_variable(var.clone(), state)
                } else {
                    default.clone()
                }
            }
            ExpansionExpr::ParameterError(var, err) => {
                eprintln!("slush: {}: {}", var, err);
                std::process::exit(1);
            }
            ExpansionExpr::ParameterAssign(var, default) => {
                if !get_variable(var.clone(), state).to_string().is_empty() {
                    get_variable(var.clone(), state)
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

#[derive(Debug, PartialEq)]
pub enum Argument {
    Name(String),
    Variable(VariableLookup),
    SubShell(SubShellExpr),
    Merge(MergeExpr),
    Expansion(ExpansionExpr),
}

impl Argument {
    fn eval(&self, state: &Arc<Mutex<State>>) -> String {
        match self {
            Argument::Name(n) => n.clone(),
            Argument::Variable(variable) => get_variable(variable.name.clone(), state),
            Argument::SubShell(ss) => ss.stdout(),
            Argument::Merge(merge) => merge.eval(state),
            Argument::Expansion(expansion) => expansion.eval(state),
        }
    }
}

// #[derive(Debug)]
// pub enum Expr {
//     //CommandExpr(CommandExpr),
//     PipeLineExpr(PipeLineExpr),
//     AssignmentExpr(AssignmentExpr),
//     //SubShellExpr(SubShellExpr)
// }

fn get_variable(var: String, state: &Arc<Mutex<State>>) -> String {
    match var.as_str() {
        "0" => String::from("slush"),
        "!" => process::id().to_string(),
        "?" => state.lock().unwrap().prev_status.to_string(),
        _ => env::var(var).unwrap_or_default(),
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

pub struct Output {
    status: Option<i32>,
    stdout: Vec<u8>,
    _stderr: Vec<u8>,
}

fn handle_jobs_cmd(opt: Option<&str>, state: &Arc<Mutex<State>>) {
    match opt {
        None => {
            let state = state.lock().expect("unable to acquire lock");
            for (job_num, _job) in state.bg_jobs.iter().enumerate() {
                println!("[{job_num}] + Running COMMAND\n")
            }
        }
        Some("-l") => {
            let state = state.lock().expect("unable to acquire lock");
            for (job_num, _job) in state.bg_jobs.iter().enumerate() {
                println!("[{job_num}] + GID Running COMMAND\n")
            }
        }
        Some("-p") => {
            let state = state.lock().expect("unable to acquire lock");
            for _job in state.bg_jobs.iter() {
                println!("PID\n")
            }
        }
        Some(opt) => {
            println!("invalid option: {opt}");
        }
    }
}

#[derive(Debug)]
pub struct Job {
    pub child: Arc<SharedChild>,
    pub cmd: CommandStr,
    pub pid: u32,
}
