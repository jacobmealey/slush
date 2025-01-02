use std::process;
use std::process::Stdio;
use std::env;


pub trait Evalable {
    // evaluate SOME command and provide a return value (0 is success, etc.) 
    fn eval(&mut self) -> i32;
}


// How do we made these outputs streams? it would be nice to have it feed between
// two child CommandExprs as they are creating them... 
#[derive(Debug)]
pub struct CommandExpr {
    pub command: process::Command,
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

impl Evalable for AssignmentExpr {
    fn eval(&mut self) -> i32 {
        unsafe{
            env::set_var(&self.key, &self.val);
        }
        0
    }
}


impl CommandExpr {
    pub fn direct_intput(&mut self) {
        self.command.stdin(Stdio::piped());
    }

    pub fn direct_output(&mut self) {
        self.command.stdout(Stdio::piped());
    }
}

impl Evalable for CommandExpr {
    fn eval(&mut self) -> i32 {
        let mut code: i32 = 0; 
        let child = match self.command.spawn() {
            Ok(c) => c,
            Err(v) => { println!("{}", v); return 2;} 
        };

        match child.wait_with_output() {
            Err(e) => { println!("{}", e)},
            Ok(o) => {
                code = o.status.code().expect("Couldn't get exit code");
            }
        }
        code
    }

}

impl Evalable for PipeLineExpr {
    fn eval(&mut self) -> i32 {
        let mut prev_child: Option<process::Child> = None;
        let sz = self.pipeline.len();
        for (i, expr) in  self.pipeline.iter_mut().enumerate() {
            if let Some(pchild) = prev_child {
                expr.direct_intput();
                expr.command.stdin(Stdio::from(pchild.stdout.unwrap()));
            }
            if i < sz - 1 {
                expr.direct_output();
            }
            prev_child = Some(match expr.command.spawn() {
                Ok(c) => c,
                Err(v) => {println!("{}", v); return 2}
            });
        }
        let exit_status = prev_child.expect("No such previous child").wait().unwrap();
        exit_status.code().expect("Couldn't get exit code from previous job")
    }
}
