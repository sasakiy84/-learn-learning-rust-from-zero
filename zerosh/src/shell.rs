use std::{sync::mpsc::{channel, sync_channel, Sender, Receiver, SyncSender}, process::exit, thread};

use crate::helper::DynError;
use nix::{
    sys::{
        signal::{killpg, signal, SigHandler, Signal},
        wait::{waitpid, WaitPidFlag, WaitStatus},
    },
    unistd::{self, dup2, execvp, fork, pipe, setpgid, tcgetpgrp, tcsetpgrp, ForkResult, Pid},
};
use rustyline::{ DefaultEditor, error::ReadlineError};
use signal_hook::{consts::*, iterator::Signals};

fn syscall<F, T>(f: F) -> Result<T, nix::Error>
where F: Fn() -> Result<T, nix::Error>
{
    loop {
        match f() {
            Err(nix::Error::EINTR) => (), // retry
            result => return result,
        }
    }
}

/// messages workder threads receive
enum WorkerMsg {
    Signal(i32), // reveive signal
    Cmd(String), // command input
}

/// message main threads receive
enum ShellMsg {
    Continue(i32), // restart shell loading. i32 is last exit code.
    Quit(i32), // terminate shell. i32 is shell exit code.
}

#[derive(Debug)]
pub struct Shell {
    logfile: String,
}

impl Shell {
    pub fn new(logfile: &str) -> Self {
        Shell { logfile: logfile.to_string() }
    }

    pub fn run(&self) -> Result<(), DynError> {
        // ignore SITTTOU, or deliver SIGTSTP
        unsafe {
            signal(Signal::SITTTOU, SigHandler::SigIgn).unwrap()
        };

        let mut rl = DefaultEditor::new()?;
        if let Err(e) = rl.load_history(&self.logfile) {
            eprint!("ZeroSh: failed to load history file: {e}");
        }

        // create channel and generate signal handler, worker thread
        let (worker_tx, worker_rx) = channel();
        let (shell_tx, shell_rx) = sync_channel(0);
        spawn_sig_handler(worker_tx.clone())?;
        Worker::new().spawn(worker_rx, shell_tx);

        let exit_val;
        let prev = 0; // previous exit code.

        loop {
            let face = if prev == 0 { '\u{1F642}' } else { '\u{1F480}' };
            // read a line, then send it to worker thread.
            match rl.readline(&format!("ZeroSh {face} %> ")) {
                Ok(line) => {
                    let line_trimed = line.trim();
                    if line_trimed.is_empty() {
                        continue;
                    } else {
                        rl.add_history_entry(line_trimed);
                    }


                    // send message to workder thread
                    worker_tx.send(WorkerMsg::Cmd(line)).unwrap();
                    match shell_rx.recv().unwrap() {
                        ShellMsg::Continue(n) => prev = n, // wait for command again.
                        ShellMsg::Quit(n) => {
                            exit_val = n;
                            break;
                        }
                    }
                }
                Err(ReadlineError::Interrupted) => eprintln!("ZeroSh: Ctrl+d to quit"),
                Err(ReadlineError::Eof) => {
                    worker_tx.send(WorkerMsg::Cmd("exit".to_string())).unwrap();
                    match shell_rx.recv().unwrap() {
                        ShellMsg::Quit(n) => {
                            exit_val = n;
                            break;
                        }
                        _ => panic!("failed to exit"),
                    }
                }
                Err(e) => {
                    eprintln!("ZeroSh: error in reading command\n{e}");
                    exit_val = 1;
                    break;
                }
            }
        }

        if let Err(e) = rl.save_history(&self.logfile) {
            eprintln!("ZeroSh: failed to write into history file: {e}");
        }

        exit(exit_val);
    }
}

fn spawn_sig_handler(tx: Sender<WorkerMsg>) -> Result<(), DynError> {
    let mut signals = Signals::new(&[SIGINT, SIGSTOP, SIGCHLD])?;
    thread::spawn(move || {
        for sig in signals.forever() {
            // receive signals and transfer it to worker thread
            tx.send(WorkerMsg::Signal(sig)).unwrap();
        }
    });

    Ok(())
}

#[derive(Debug)]
struct Worker {}

impl Worker {
    fn new() -> Self {
        todo!()
    }

    fn spawn(&self, worker_rx: Receiver<WorkerMsg>, shell_tx: SyncSender<ShellMsg>) {
        todo!()
    }
}

