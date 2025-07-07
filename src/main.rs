use crate::expr::Argument;
use crate::parser::tokenizer;
use nix::sys::signal;
use std::env;
use std::io::{self, BufRead, Write};
use std::panic;
use std::rc::Rc;
use std::sync::{Arc, LazyLock, Mutex};
mod expr;
mod parser;

static PRUNE_JOBS: LazyLock<Arc<Mutex<bool>>> =
    std::sync::LazyLock::new(|| Arc::new(Mutex::new(false)));

pub extern "C" fn sigchld_handler(_signum: i32) {
    let mut jobs = PRUNE_JOBS.lock().unwrap();
    *jobs = true;
}

fn repl() {
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let state = expr::State::new();

    panic::set_hook(Box::new(|panic_info| {
        println!("one or more internal error occurred while running slush.");
        println!("{}", panic_info);
        println!("Submit an issue (or a pull request!) here: https://github.com/jacobmealey/slush");
    }));

    unsafe {
        signal::signal(
            signal::Signal::SIGCHLD,
            signal::SigHandler::Handler(sigchld_handler),
        )
        .expect("Error setting signal handler");
    }
    if let Some(arg) = env::args().nth(1) {
        let code_str = std::fs::read_to_string(arg).expect("Error reading file");
        let s = state.clone();
        {
            let passed_args: Vec<String> = env::args().collect();
            let script_args = &mut s.borrow_mut().argstack;
            script_args.borrow_mut().push(Rc::new(
                passed_args
                    .into_iter()
                    .skip(2) // skip the slush exec and name of script.
                    .map(Argument::Name)
                    .collect::<Vec<Argument>>(),
            ));
        }
        let mut parser = parser::Parser::new(s.clone());
        parser.parse(&code_str);
        if !parser.err.is_empty() {
            println!("{}", parser.err);
            return;
        } else {
            for mut expr in parser.exprs {
                s.borrow_mut().prev_status = expr.eval();
            }
        }
        std::process::exit(state.borrow().prev_status);
    } else {
        println!("Hello, Slush!");
        let s = state.clone();
        loop {
            if *PRUNE_JOBS.lock().unwrap() {
                let mut jobs = state.borrow_mut();
                jobs.fg_jobs
                    .retain(|job| job.child.try_wait().unwrap().is_some());
                *PRUNE_JOBS.lock().unwrap() = false;
            }
            print!("[{}] $ ", state.borrow().prev_status);
            stdout.flush().expect("Error flushing to stdout");
            let line = match stdin.lock().lines().next() {
                Some(Ok(line)) => line,
                _ => break,
            };
            let mut parser = parser::Parser::new(state.clone());
            parser.parse(&line);
            if !parser.err.is_empty() {
                println!("{}", parser.err);
                continue;
            } else {
                for expr in &mut parser.exprs {
                    s.borrow_mut().prev_status = expr.eval();
                }
            }

            s.borrow_mut().fg_jobs.clear();
        }
    }
}

fn main() {
    repl();
}
