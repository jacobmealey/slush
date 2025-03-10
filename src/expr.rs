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
    pub fg_jobs: Vec<Arc<SharedChild>>,
    pub bg_jobs: Vec<Arc<SharedChild>>,
}

impl State {
    pub fn new() -> Arc<Mutex<State>> {
        Arc::new(Mutex::new(State {
            bg_jobs: Vec::new(),
            fg_jobs: Vec::new(),
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
pub struct IfExpr {
    pub condition: PipeLineExpr,
    pub if_branch: Vec<PipeLineExpr>,
    pub else_branch: Option<Vec<PipeLineExpr>>,
}

impl IfExpr {
    pub fn eval(&mut self) -> i32 {
        if self.condition.eval() == 0 {
            for command in &mut self.if_branch {
                command.eval();
            }
        } else if let Some(else_branch) = &mut self.else_branch {
            for command in else_branch {
                command.eval();
            }
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
    fn eval(&mut self) -> i32 {
        unsafe {
            env::set_var(&self.key, self.val.eval());
        }
        0
    }
}

impl CommandExpr {
    pub fn build_command(&self) -> Box<process::Command> {
        let com = self.command.eval();
        let mut cmd = Box::new(Command::new(&com));
        for arg in &self.arguments {
            cmd.arg(arg.eval());
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
                        ass.eval();
                    }

                    if let Argument::Name(arg) = &exp.command {
                        if arg.is_empty() {
                            continue;
                        }
                    }

                    let base_command = exp.command.eval();
                    // should built ins be there own special node on the tree?
                    if base_command == "cd" {
                        return change_dir::ChangeDir::new(&exp.arguments[0].eval()).eval();
                    } else if base_command == "true" {
                        return 0;
                    } else if base_command == "false" {
                        return 1;
                    } else if base_command == "exit" {
                        if !exp.arguments.is_empty() {
                            std::process::exit(exp.arguments[0].eval().parse().unwrap_or_default());
                        } else {
                            std::process::exit(0);
                        }
                    }

                    let mut cmd = exp.build_command();

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
                                    exp.command.eval(),
                                    v
                                );
                                return 2;
                            }
                        },
                        Err(v) => {
                            println!("Error spawning {}: {}", exp.command.eval(), v);
                            return 2;
                        }
                    });
                    if self.background {
                        state.bg_jobs.push(prev_child.as_ref().unwrap().clone());
                    } else {
                        state.fg_jobs.push(prev_child.as_ref().unwrap().clone());
                    }
                    0
                }
            };
        }
        let mut exit_status: i32 = 0;
        if let Some(rcstr) = &self.capture_out {
            if !self.background {
                let outie = wait_with_output(&prev_child.unwrap()).expect("Nothing");
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
            let filename = self.file_redirect.as_ref().unwrap().eval();
            let mut file = match File::create(filename) {
                Ok(f) => f,
                Err(_) => return 1,
            };
            let outie = wait_with_output(&prev_child.unwrap()).expect("Nothing");
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
    pub fn eval(&self) -> String {
        self.left.eval() + &self.right.eval()
    }
}

#[derive(Debug, PartialEq)]
pub enum Argument {
    Name(String),
    Variable(VariableLookup),
    SubShell(SubShellExpr),
    Merge(MergeExpr),
}

impl Argument {
    fn eval(&self) -> String {
        match self {
            Argument::Name(n) => n.clone(),
            Argument::Variable(variable) => get_variable(variable.name.clone()),
            Argument::SubShell(ss) => ss.stdout(),
            Argument::Merge(merge) => merge.eval(),
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

fn get_variable(var: String) -> String {
    env::var(var).unwrap_or_default()
}

// Todo: clean up error mapping.
fn wait_with_output(child: &SharedChild) -> Result<Output, usize> {
    drop(child.take_stdin());

    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    match (child.take_stdout(), child.take_stderr()) {
        (None, None) => {}
        (Some(mut out), None) => {
            let res = out.read_to_end(&mut stdout);
            res.map_err(|_| 1usize)?;
        }
        (None, Some(mut err)) => {
            let res = err.read_to_end(&mut stderr);
            res.map_err(|_| 1usize)?;
        }
        (Some(mut out), Some(mut err)) => {
            let out_handle = std::thread::spawn(move || {
                let _ = out.read_to_end(&mut stdout);
                stdout
            });
            let err_handle = std::thread::spawn(move || {
                let _ = err.read_to_end(&mut stderr);
                stderr
            });

            stdout = out_handle.join().map_err(|_| 1usize)?;
            stderr = err_handle.join().map_err(|_| 1usize)?;
        }
    }

    let status = child.wait().map_err(|_| 1usize)?;
    Ok(Output {
        status: status.code(),
        stdout,
        stderr,
    })
}

pub struct Output {
    status: Option<i32>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}
