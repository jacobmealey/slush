use std::process;
use std::process::Command;
use std::process::Stdio;
use std::env;


pub trait Evalable {
    // evaluate SOME command and provide a return value (0 is success, etc.) 
    fn eval(&mut self) -> i32;
}


#[derive(Debug)]
pub struct VariableLookup {
    pub name: String
}


// How do we made these outputs streams? it would be nice to have it feed between
// two child CommandExprs as they are creating them... 
#[derive(Debug)]
pub struct CommandExpr {
    pub command: Argument,
    pub arguments: Vec<Argument>
}

#[derive(Debug)]
pub struct PipeLineExpr {
    pub pipeline: Vec<CommandExpr>
}

#[derive(Debug)]
pub struct AssignmentExpr {
    pub key: String,
    pub val: String
}

// Subshell is simply a wrapper around a string which can be fed into a parser,
// evaluated and stdout returned.
// #[derive(Debug)]
// pub struct SubShellExpr {
//     pub shell: String
// }

impl Evalable for AssignmentExpr {
    fn eval(&mut self) -> i32 {
        unsafe{
            env::set_var(&self.key, &self.val);
        }
        0
    }
}

impl CommandExpr {
    pub fn build_command(&self) -> Box<process::Command> {
        let mut cmd = Box::new(Command::new(match &self.command {
            Argument::Name(name) => name.clone(),
            Argument::Variable(variable) => get_variable(variable.name.clone())
        }));

        for arg in &self.arguments {
            cmd.arg(match arg {
                Argument::Name(n) => n.clone(),
                Argument::Variable(variable) => get_variable(variable.name.clone())
            });
        }
        cmd
    }
}

impl Evalable for PipeLineExpr {
    fn eval(&mut self) -> i32 {
        let mut prev_child: Option<process::Child> = None;
        let sz = self.pipeline.len();
        for (i, expr) in  self.pipeline.iter_mut().enumerate() {
            let mut cmd = expr.build_command();
            if let Some(pchild) = prev_child {
                cmd.stdin(Stdio::from(pchild.stdout.unwrap()));
            }
            if i < sz - 1 {
                cmd.stdout(Stdio::piped());
            }
            prev_child = Some(match cmd.spawn() {
                Ok(c) => c,
                Err(v) => {println!("{}", v); return 2}
            });
        }
        let exit_status = prev_child.expect("No such previous child").wait().unwrap();
        exit_status.code().expect("Couldn't get exit code from previous job")
    }
}

#[derive(Debug)]
pub enum Argument {
    Name(String),
    Variable(VariableLookup)
}

// #[derive(Debug)]
// pub enum Expr {
//     CommandExpr(CommandExpr),
//     PipeLineExpr(PipeLineExpr),
//     AssignmentExpr(AssignmentExpr),
//     SubShellExpr(SubShellExpr)
// }

fn get_variable(var: String) -> String {
    env::var(var).expect("")
}
