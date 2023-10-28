//! # Command-Line Help for `sesh`
//!
//! This document contains the help content for the `sesh` command-line program.
//!
//! **Command Overview:**
//!
//! * [`sesh`↴](#sesh)
//! * [`sesh resume`↴](#sesh-resume)
//! * [`sesh start`↴](#sesh-start)
//! * [`sesh attach`↴](#sesh-attach)
//! * [`sesh select`↴](#sesh-select)
//! * [`sesh detach`↴](#sesh-detach)
//! * [`sesh kill`↴](#sesh-kill)
//! * [`sesh list`↴](#sesh-list)
//! * [`sesh shutdown`↴](#sesh-shutdown)
//!
//! ## `sesh`
//!
//! A terminal session manager for unix systems. Run persistent, named tasks that you can detach from and attach to at any time - both on your local machine, and over SSH
//!
//! **Usage:** `sesh [OPTIONS] [PROGRAM] [ARGS]... [COMMAND]`
//!
//! ###### **Subcommands:**
//!
//! * `resume` — Resume the last used session [alias: r]
//! * `start` — Start a new session, optionally specifying a name [alias: s]
//! * `attach` — Attach to a session [alias: a]
//! * `select` — Fuzzy select a session to attach to [alias: f]
//! * `detach` — Detach from a session [alias: d]
//! * `kill` — Kill a session [alias: k]
//! * `list` — List sessions [alias: ls]
//! * `shutdown` — Shutdown the server (kill all sessions)
//!
//! ###### **Arguments:**
//!
//! * `<PROGRAM>`
//! * `<ARGS>`
//!
//! ###### **Options:**
//!
//! * `-n`, `--name <NAME>`
//! * `-d`, `--detached`
//!
//!
//!
//! ## `sesh resume`
//!
//! Resume the last used session [alias: r]
//!
//! Specify --create / -c to create a new session if one does not exist
//!
//! **Usage:** `sesh resume [OPTIONS]`
//!
//! ###### **Options:**
//!
//! * `-c`, `--create` — Create a new session if one does not exist
//!
//!
//!
//! ## `sesh start`
//!
//! Start a new session, optionally specifying a name [alias: s]
//!
//! If no program is specified, the default shell will be used.
//! If no name is specified, the name will be [program name]-[n-1] where n is the number of sessions
//! with that program name.
//! If --detached / -d is present, the session will not be attached to the client on creation
//! and will run in the background.
//!
//! **Usage:** `sesh start [OPTIONS] [PROGRAM] [ARGS]...`
//!
//! ###### **Arguments:**
//!
//! * `<PROGRAM>`
//! * `<ARGS>`
//!
//! ###### **Options:**
//!
//! * `-n`, `--name <NAME>`
//! * `-d`, `--detached`
//!
//!
//!
//! ## `sesh attach`
//!
//! Attach to a session [alias: a]
//!
//! Select a session by index or name.
//! If --create / -c is present, a new session will be created if one does not exist.
//! If the session was selected by name and the session was not present, the new session
//! created by --create will have the specified name.
//!
//! **Usage:** `sesh attach [OPTIONS] <SESSION>`
//!
//! ###### **Arguments:**
//!
//! * `<SESSION>` — Id or name of session
//!
//! ###### **Options:**
//!
//! * `-c`, `--create` — Create a new session if one does not exist
//!
//!
//!
//! ## `sesh select`
//!
//! Fuzzy select a session to attach to [alias: f]
//!
//! Opens a fuzzy selection window provided by the dialoguer crate.
//! Type to fuzzy find files, or use the Up/Down arrows to navigate.
//! Press Enter to confirm your selection, or Escape to cancel.
//!
//! **Usage:** `sesh select`
//!
//!
//!
//! ## `sesh detach`
//!
//! Detach from a session [alias: d]
//!
//! If no session is specified, detaches from the current session (if it exists).
//! Otherwise, detaches the specified session from its owning client.
//!
//! **Usage:** `sesh detach [SESSION]`
//!
//! ###### **Arguments:**
//!
//! * `<SESSION>` — Id or name of session
//!
//!
//!
//! ## `sesh kill`
//!
//! Kill a session [alias: k]
//!
//! Kills a session and the process it owns.
//! Select a session by name or index.
//!
//! **Usage:** `sesh kill <SESSION>`
//!
//! ###### **Arguments:**
//!
//! * `<SESSION>` — Id or name of session
//!
//!
//!
//! ## `sesh list`
//!
//! List sessions [alias: ls]
//!
//! Prints a compact list of session names and indexes.
//! With the --info / -i option, prints a nicely formatted table with info about each session.
//!
//! **Usage:** `sesh list [OPTIONS]`
//!
//! ###### **Options:**
//!
//! * `-i`, `--info` — Print detailed info about sessions
//!
//!
//!
//! ## `sesh shutdown`
//!
//! Shutdown the server (kill all sessions)
//!
//! **Usage:** `sesh shutdown`

use std::{path::PathBuf, process::ExitCode};

use clap::Parser;
use libc::exit;
use sesh_cli::{Cli, Command};
use sesh_shared::{pty::Pty, term::Size};
use session::Ctx;
use termion::{
    color::{Color, Fg},
    style::Bold,
};
use tokio::sync::broadcast;

use sesh_proto::sesh_cli_server::SeshCli;

mod session;

#[repr(u8)]
#[derive(Debug, Clone)]
enum ExitKind {
    Quit,
    Detach,
}

/// Formats the given input as green, then resets
#[macro_export]
macro_rules! success {
    ($($arg:tt)*) => {
        format!(
            "{}{}{}",
            termion::color::Fg(termion::color::Green),
            format!($($arg)*),
            termion::color::Fg(termion::color::Reset)
        )
    };
}

/// Formats the given input as red, then resets
#[macro_export]
macro_rules! error {
    ($($arg:expr),*) => {
        format!(
            "{}{}{}",
            termion::color::Fg(termion::color::Red),
            format!($($arg),*),
            termion::color::Fg(termion::color::Reset)
        )
    };
}

#[derive(Clone)]
/// Server -> Client connection service
struct SeshCliService {
    exit_tx: broadcast::Sender<ExitKind>,
}

#[tonic::async_trait]
impl SeshCli for SeshCliService {
    /// Server -> Client request to detach a session
    async fn detach(
        &self,
        _: tonic::Request<sesh_proto::ClientDetachRequest>,
    ) -> std::result::Result<tonic::Response<sesh_proto::ClientDetachResponse>, tonic::Status> {
        self.exit_tx
            .send(ExitKind::Detach)
            .map_err(|_| tonic::Status::internal("Failed to send exit signal to client"))?;
        Ok(tonic::Response::new(sesh_proto::ClientDetachResponse {}))
    }
}

fn get_program(program: Option<String>) -> String {
    program.unwrap_or_else(|| std::env::var("SHELL").unwrap_or("bash".to_owned()))
}

/// Formats an icon and title pair, giving the icon its own color
fn icon_title<T: Color>(icon: char, title: &str, icon_color: Fg<T>) -> String {
    format!(
        "{}{}{} {}{}{}",
        icon_color,
        icon,
        Fg(termion::color::Reset),
        Bold,
        title,
        termion::style::Reset
    )
}

enum ListMode {
    List,
    Table,
    Json,
}

impl ListMode {
    pub fn new(table: bool, json: bool) -> Self {
        if json {
            Self::Json
        } else if table {
            Self::Table
        } else {
            Self::List
        }
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();

    let rt = dirs::runtime_dir()
        .unwrap_or(PathBuf::from("/tmp/"))
        .join("sesh/");
    let server_sock = rt.join("server.sock");

    let cmd = match cli.command {
        Some(cmd) => cmd,
        None => Command::Start {
            name: cli.args.name,
            program: cli.args.program,
            args: cli.args.args,
            detached: cli.args.detached,
        },
    };
    if !server_sock.exists() {
        if matches!(cmd, Command::Shutdown)
            || matches!(cmd, Command::List { .. })
            || matches!(cmd, Command::Kill { .. })
        {
            println!("{}", success!("[not running]"));
            return ExitCode::SUCCESS;
        } else {
            let size = Size::term_size().unwrap_or(Size { cols: 80, rows: 24 });
            if unsafe { libc::fork() == 0 } {
                let res = Pty::builder(std::env::var("SESHD_PATH").unwrap_or("seshd".to_owned()))
                    .daemonize()
                    .env("RUST_LOG", "INFO")
                    .spawn(&size);
                unsafe {
                    match res {
                        Ok(_) => exit(0),
                        Err(_) => exit(1),
                    }
                }
            }
            let now = std::time::Instant::now();
            while !server_sock.exists() {
                std::thread::sleep(std::time::Duration::from_millis(100));
                if now.elapsed().as_secs() > 5 {
                    eprintln!("{}", error!("[failed to connect to server]"));
                    return ExitCode::FAILURE;
                }
            }
        }
    }

    let Ok(ctx) = Ctx::init(server_sock).await else {
        eprintln!("{}", error!("[failed to connect to server]"));
        return ExitCode::FAILURE;
    };

    let message = match cmd {
        Command::Start {
            name,
            program,
            args,
            detached,
        } => session::start(ctx, name, program, args, !detached).await,
        Command::Resume { create } => session::resume(ctx, create).await,
        Command::Attach { session, create } => session::attach(ctx, session, create).await,
        Command::Kill { session } => session::kill(ctx, session).await,
        Command::Detach { session } => session::detach(ctx, session).await,
        Command::Select => session::select(ctx).await,
        Command::List { info, json } => session::list(ctx, info, json).await,
        Command::Shutdown => session::shutdown(ctx).await,
    };

    match message {
        Ok(Some(message)) => println!("{}", message),
        Ok(None) => (),
        Err(e) => {
            println!("{}", error!("{}", e));
            return ExitCode::FAILURE;
        }
    }

    unsafe { exit(0) };
}
