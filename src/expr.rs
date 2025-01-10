use crate::parser::Parser;
use std::env;
use std::process;
use std::process::Command;
use std::process::Stdio;


pub trait Evalable {
    // evaluate SOME command and provide a return value (0 is success, etc.)
    fn eval(&mut self) -> i32;
}

#[derive(Debug)]
pub struct VariableLookup {
    pub name: String,
}

// How do we made these outputs streams? it would be nice to have it feed
// between two child CommandExprs as they are creating them...
#[derive(Debug)]
pub struct CommandExpr {
    pub command: Argument,
    pub arguments: Vec<Argument>,
}

#[derive(Debug)]
pub struct PipeLineExpr {
    pub pipeline: Vec<CommandExpr>,
}

// #[derive(Debug)]
// pub struct AndIfLeaf {
//     pub left: PipeLineExpr,
//     pub right: PipeLineExpr,
// }


#[derive(Debug)]
pub enum AndOrNode {
    Pipeline(Box<PipeLineExpr>),
    Andif(Box<AndIf>),
    Orif(Box<OrIf>)
}

impl AndOrNode{
    fn eval(&mut self) -> i32 {
       match self {
           AndOrNode::Pipeline(pl) => pl.eval(),
           AndOrNode::Andif(and) => and.eval(),
           AndOrNode::Orif(or) => or.eval(),
       }
    }
}

#[derive(Debug)]
pub struct OrIf {
    left: AndOrNode,
    right: AndOrNode
}

impl OrIf {
    fn eval(&mut self) -> i32 {
        let ll = self.left.eval();
        if ll != 0 {
            return self.right.eval();
        }
        ll
    }
}

#[derive(Debug)]
pub struct AndIf {
    left: AndOrNode,
    right: AndOrNode
}

impl AndIf {
    fn eval(&mut self) -> i32 {
        let ll = self.left.eval();
        let rr = self.right.eval();
        if ll == 0 && rr == 0 {
            return ll;
        }
        rr
    }
}

// pub struct And IF
#[derive(Debug)]
pub struct AssignmentExpr {
    pub key: String,
    pub val: Argument,
}

// Subshell is simply a wrapper around a string which can be fed into a
// parser, evaluated and stdout returned.
#[derive(Debug)]
pub struct SubShellExpr {
    pub shell: String,
}

impl SubShellExpr {
    pub fn stdout(&self) -> String {
        let mut parser = Parser::new(&self.shell);
        let mut shell_output: String = Default::default();
        parser.parse();
        for expr in parser.exprs {
            match expr {
                Expr::PipeLineExpr(mut pl) => pl.run_with_out(&mut shell_output),
                Expr::AssignmentExpr(mut ass) => ass.eval(),
            };
        }
        shell_output
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
            let mut cmd = expr.build_command();
            if let Some(pchild) = prev_child {
                cmd.stdin(Stdio::from(pchild.stdout.unwrap()));
            }
            if i < sz - 1 {
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
        let exit_status = prev_child.expect("No such previous child").wait().unwrap();
        exit_status
            .code()
            .expect("Couldn't get exit code from previous job")
    }
}

impl PipeLineExpr {
    // how to unduplicated
    fn run_with_out(&mut self, output: &mut String) -> i32 {
        let mut prev_child: Option<process::Child> = None;
        for expr in self.pipeline.iter_mut() {
            let mut cmd = expr.build_command();
            if let Some(pchild) = prev_child {
                cmd.stdin(Stdio::from(pchild.stdout.unwrap()));
            }
            cmd.stdout(Stdio::piped());
            prev_child = Some(match cmd.spawn() {
                Ok(c) => c,
                Err(v) => {
                    println!("{}", v);
                    return 2;
                }
            });
        }

        let outie = prev_child
            .expect("No Child Process")
            .wait_with_output()
            .expect("Nothing");
        *output = String::from_utf8(outie.stdout).unwrap();
        // trim trailing newlines if its an issue
        if output.ends_with('\n') {
            output.pop();
        }
        outie
            .status
            .code()
            .expect("Couldn't get exit code from previous job")
    }
}


#[derive(Debug)]
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

#[derive(Debug)]
pub enum Expr {
    //CommandExpr(CommandExpr),
    PipeLineExpr(PipeLineExpr),
    AssignmentExpr(AssignmentExpr),
    //SubShellExpr(SubShellExpr)
}

fn get_variable(var: String) -> String {
    env::var(var).expect("")
}
