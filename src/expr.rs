use std::process;
use std::io::Write;


pub trait Evalable {
    // evaluate SOME command and provide a return value (0 is success, etc.) 
    fn eval(&mut self) -> i32;
    fn pipe_in(&mut self, input: String);
    fn pipe_out(&self) -> String;
}


// How do we made these outputs streams? it would be nice to have it feed between
// two child CommandExprs as they are creating them... 
pub struct CommandExpr {
    pub command: process::Command,
    pub output: String,
    pub input: String,
}

pub struct PipeLineExpr {
    pub pipeline: Vec<Box<dyn Evalable>>
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

    fn pipe_out(&self) -> String {
       self.output.clone()
    }

}

impl Evalable for PipeLineExpr {
    fn eval(&mut self) -> i32 {
        let mut lastcode = 0;
        let mut prev_expr: Option<&mut Box<dyn Evalable>> = None; 
        for expr in &mut self.pipeline {
            if let Some(pexpr) = prev_expr {
                expr.pipe_in(pexpr.pipe_out());
            }
            lastcode = expr.eval();
            prev_expr = Some(expr);
        }
        print!("{}", self.pipeline.last().expect("No such lat element").pipe_out());
        lastcode
    }
    fn pipe_in(&mut self, input: String) {
        if self.pipeline.len() > 1 {
            self.pipeline[0].pipe_in(input);
        }
    }

    fn pipe_out(&self) -> String {
        if self.pipeline.len() > 1 {
            return self.pipeline.last().expect("No such last element").pipe_out();
        }
        "".to_string()
    }
}
