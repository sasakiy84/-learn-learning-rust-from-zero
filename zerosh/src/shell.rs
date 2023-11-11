use crate::helper::DynError;
use nix::{
    libc,
    sys::{
        signal::{killpg, signal, SigHandler, Signal},
        wait::{waitpid, WaitPidFlag, WaitStatus},
    },
    unistd::{self, dup2, execvp, fork, pipe, setpgid, tcgetpgrp, tcsetpgrp, ForkResult, Pid},
};
use rustyline::{error::ReadlineError, DefaultEditor};
use signal_hook::{consts::*, iterator::Signals};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    ffi::CString,
    mem::replace,
    path::PathBuf,
    process::exit,
    sync::mpsc::{channel, sync_channel, Receiver, Sender, SyncSender},
    thread,
};

fn syscall<F, T>(f: F) -> Result<T, nix::Error>
where
    F: Fn() -> Result<T, nix::Error>,
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
    Quit(i32),     // terminate shell. i32 is shell exit code.
}

#[derive(Debug)]
pub struct Shell {
    logfile: String,
}

impl Shell {
    pub fn new(logfile: &str) -> Self {
        Shell {
            logfile: logfile.to_string(),
        }
    }

    pub fn run(&self) -> Result<(), DynError> {
        // ignore SITTTOU, or deliver SIGTSTP
        unsafe { signal(Signal::SITTTOU, SigHandler::SigIgn).unwrap() };

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

#[derive(Debug, PartialEq, Eq, Clone)]
enum ProcState {
    Run,
    Stop,
}

#[derive(Debug, Clone)]
struct ProcInfo {
    state: ProcState,
    gpid: Pid,
}

#[derive(Debug)]
struct Worker {
    exit_val: i32,
    fg: Option<Pid>, // foreground process group id

    jobs: BTreeMap<usize, (Pid, String)>, // mappping job id to (process group id, command)

    pgid_top_pids: HashMap<Pid, (usize, HashSet<Pid>)>, // mapping process group id to (job id,
    // process id)
    pid_to_info: HashMap<Pid, ProcInfo>,
    shell_pgid: Pid,
}

impl Worker {
    fn new() -> Self {
        Worker {
            exit_val: 0,
            fg: None, // foreground is shell
            jobs: BTreeMap::new(),
            pgid_top_pids: HashMap::new(),
            pid_to_info: HashMap::new(),
            shell_pgid: tcgetpgrp(libc::STDIN_FILENO).unwrap(),
        }
    }

    fn spawn(&self, worker_rx: Receiver<WorkerMsg>, shell_tx: SyncSender<ShellMsg>) {
        thread::spawn(move || {
            for msg in worker_rx.iter() {
                match msg {
                    WorkerMsg::Cmd(line) => {
                        match parse_cmd(&line) {
                            Ok(cmd) => {
                                if self.build_in_cmd(&cmd, &shell_tx) {
                                    // if buildin cmd, receive from worker_rx
                                    continue;
                                }

                                // if failed to spawn child process, wait input again
                                if !self.spawn_child(&line, &cmd) {
                                    shell_tx.send(ShellMsg::Continue(self.exit_val)).unwrap();
                                }
                            }
                            Err(e) => {
                                eprintln!("ZeroSh: {e}");
                                shell_tx.send(ShellMsg::Continue(self.exit_val)).unwrap();
                            }
                        }
                    }
                    WorkerMsg::Signal(SIGCHLD) => self.wait_child(&shell_tx),
                    _ => (), // do nothing
                }
            }
        });
    }

    fn wait_child(&self, shell_tx: &SyncSender<ShellMsg>) {
        todo!();
    }

    fn build_in_cmd(&mut self, cmd: &[(&str, Vec<&str>)], shell_tx: &SyncSender<ShellMsg>) -> bool {
        todo!()
    }

    fn spawn_child(&mut self, line: &str, cmd: &[(&str, Vec<&str>)]) -> bool {
        todo!()
    }
}

type CmdResult<'a> = Result<Vec<(&'a str, Vec<&'a str>)>, DynError>;
fn parse_cmd(line: &str) -> CmdResult {
    todo!()
}
