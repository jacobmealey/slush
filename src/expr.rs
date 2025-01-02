use std::process;
use std::io::Write;
use std::process::Stdio;


pub trait Evalable {
    // evaluate SOME command and provide a return value (0 is success, etc.) 
    fn eval(&mut self) -> i32;
    fn pipe_in(&mut self, input: String);
    fn pipe_out(&mut self) -> String;
}


// How do we made these outputs streams? it would be nice to have it feed between
// two child CommandExprs as they are creating them... 
pub struct CommandExpr {
    pub command: process::Command,
    pub output: String,
    pub input: String,
}

pub struct PipeLineExpr {
    pub pipeline: Vec<Box<CommandExpr>>
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
        let mut child = match self.command.spawn() {
            Ok(c) => c,
            Err(v) => { println!("{}", v); return 2;} 
        };

        {
            if !self.input.is_empty() {
                let mut stdin = child.stdin.take().unwrap();
                let _ = stdin.write(self.input.as_bytes());
            }
        }
        match child.wait_with_output() {
            Err(e) => { println!("{}", e)},
            Ok(o) => {
                code = o.status.code().expect("Couldn't get exit code");
                self.output = String::from_utf8(o.stdout.clone()).unwrap();
            }
        }
        code
    }

    fn pipe_in(&mut self, input: String) {
        self.input = input;
    }

    fn pipe_out(&mut self) -> String {
       self.output.clone()
    }

}

impl Evalable for PipeLineExpr {
    fn eval(&mut self) -> i32 {
        let mut lastcode = 0;
        let mut prev_expr: Option<&mut Box<CommandExpr>> = None; 
        let mut prev_child: Option<process::Child> = None;
        let sz = self.pipeline.len();
        for (i, expr) in  self.pipeline.iter_mut().enumerate() {
            if let Some(pexpr) = &mut prev_expr {
                expr.direct_intput();
                expr.command.stdin(Stdio::from(prev_child.unwrap().stdout.unwrap()));
            }
            if i < sz - 1 {
                expr.direct_output();
            }
            prev_child = Some(expr.command.spawn().unwrap());
            //lastcode = expr.eval();
            prev_expr = Some(expr);
        }
        let exit_status = prev_child.expect("No such previous child").wait().unwrap();
        exit_status.code().expect("Couldn't get exit code from previous job")
    }
    fn pipe_in(&mut self, input: String) {
        if self.pipeline.len() > 1 {
            self.pipeline[0].pipe_in(input);
        }
    }

    fn pipe_out(&mut self) -> String {
        if self.pipeline.len() > 1 {
            return self.pipeline.last_mut().expect("No such last element").pipe_out();
        }
        "".to_string()
    }
}
