use crate::parser::Parser;
use std::env;
use std::process;
use std::process::Command;
use std::process::Stdio;
use std::cell::RefCell;
use std::rc::Rc;


pub trait Evalable {
    // evaluate SOME command and provide a return value (0 is success, etc.)
    fn eval(&mut self) -> i32;
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

#[derive(Debug, PartialEq)]
pub struct PipeLineExpr {
    pub pipeline: Vec<CommandExpr>,
    pub capture_out: Option<Rc<RefCell<String>>>
}

#[derive(Debug, PartialEq)]
pub enum AndOrNode {
    Pipeline(Box<PipeLineExpr>),
    Andif(Box<AndIf>),
    Orif(Box<OrIf>)
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
    pub right: AndOrNode
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
    pub right: AndOrNode
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
        let mut parser = Parser::new(&self.shell);
        let shell_output: Rc<RefCell<String>> = Default::default();
        parser.parse();
        for mut expr in parser.exprs {
            expr.set_output_capture(shell_output.clone());
            expr.eval();
        }
        let x = shell_output.borrow().clone(); x
    }
}

impl Evalable for AssignmentExpr {
    fn eval(&mut self) -> i32 {
        unsafe {
            env::set_var(&self.key, self.val.eval());
        }
        0
    }
}

impl CommandExpr {
    pub fn build_command(&self) -> Box<process::Command> {
        let mut cmd = Box::new(Command::new(self.command.eval()));
        for arg in &self.arguments {
            cmd.arg(arg.eval());
        }
        cmd
    }
}

impl Evalable for PipeLineExpr {
    fn eval(&mut self) -> i32 {
        let mut prev_child: Option<process::Child> = None;
        let sz = self.pipeline.len();
        for (i, expr) in self.pipeline.iter_mut().enumerate() {
            if let Some(ref mut ass) = expr.assignment {
                ass.eval();
            }

            if let Argument::Name(arg) = &expr.command {
                if arg.is_empty() {
                    continue;
                }
            }

            let mut cmd = expr.build_command();

            if let Some(pchild) = prev_child {
                cmd.stdin(Stdio::from(pchild.stdout.unwrap()));
            }
            if i < sz - 1 || self.capture_out.is_some() {
                cmd.stdout(Stdio::piped());
            }
            prev_child = Some(match cmd.spawn() {
                Ok(c) => c,
                Err(v) => {
                    println!("{}", v);
                    return 2;
                }
            });
        }
        let mut exit_status: i32 = 0; 
        if let Some(rcstr) = &self.capture_out {
            let outie = prev_child
                .expect("No Child Process")
                .wait_with_output()
                .expect("Nothing");
            rcstr.borrow_mut().push_str(&String::from_utf8(outie.stdout.clone()).unwrap());
            if rcstr.borrow().ends_with('\n') {
                rcstr.borrow_mut().pop();
            }
            exit_status = outie.status.code().expect("Couldn't get exit code from prev job");
        } else if prev_child.is_some()  {
            let status = prev_child.expect("No such previous child").wait().unwrap();
            exit_status = status 
                .code()
                .expect("Couldn't get exit code from previous job");
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
pub enum Argument {
    Name(String),
    Variable(VariableLookup),
    SubShell(SubShellExpr),
}

impl Argument {
    fn eval(&self) -> String {
        match self {
            Argument::Name(n) => n.clone(),
            Argument::Variable(variable) => get_variable(variable.name.clone()),
            Argument::SubShell(ss) => ss.stdout(),
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
    env::var(var).expect("")
}
