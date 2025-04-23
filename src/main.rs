use crate::parser::tokenizer;
use nix::sys::signal;
use std::env;
use std::io::{self, BufRead, Write};
use std::panic;
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
        println!("Panic Info: {}", panic_info);
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
        let mut parser = parser::Parser::new(s.clone());
        parser.parse(&code_str);
        if !parser.err.is_empty() {
            println!("{}", parser.err);
            return;
        } else {
            for mut expr in parser.exprs {
                s.lock().unwrap().prev_status = expr.eval();
            }
        }
        std::process::exit(state.clone().lock().unwrap().prev_status);
    } else {
        println!("Hello, Slush!");
        let s = state.clone();
        let handler_state = state.clone();
        ctrlc::set_handler(move || {
            for job in &mut handler_state.lock().expect("Could not unlock jobs").fg_jobs {
                job.child.kill().unwrap();
            }
            println!();
            handler_state
                .lock()
                .expect("Could not unlock jobs")
                .fg_jobs
                .clear();
        })
        .expect("Error ignoring control C");
        loop {
            if *PRUNE_JOBS.lock().unwrap() {
                let mut jobs = state.lock().unwrap();
                jobs.fg_jobs
                    .retain(|job| job.child.try_wait().unwrap().is_some());
                *PRUNE_JOBS.lock().unwrap() = false;
            }
            print!("[{}] $ ", state.lock().unwrap().prev_status);
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
                    s.lock().unwrap().prev_status = expr.eval();
                }
            }

            s.lock().expect("Could not unlock jobs").fg_jobs.clear();
        }
    }
}

fn main() {
    repl();
}
