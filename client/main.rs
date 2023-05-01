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

use std::{
    io::{Cursor, Read, Write},
    path::PathBuf,
    process::ExitCode,
};

use anyhow::{Context, Result};
use chrono::{Local, TimeZone};
use clap::Parser;
use dialoguer::theme;
use libc::exit;
use prettytable::{
    format::{FormatBuilder, LinePosition, LineSeparator},
    row, Table,
};
use sesh_cli::{Cli, Command, SessionSelector};
use sesh_shared::{pty::Pty, term::Size};
use termion::{
    color::{self, Color, Fg},
    get_tty,
    raw::IntoRawMode,
    screen::IntoAlternateScreen,
    style::Bold,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
    select,
    signal::unix::{self, signal, SignalKind},
    sync::broadcast,
};
use tokio_stream::wrappers::UnixListenerStream;
use tonic::transport::Server as RPCServer;
use tonic::transport::{Channel, Endpoint, Uri};
use tower::service_fn;

use sesh_proto::{
    sesh_cli_server::{SeshCli, SeshCliServer},
    sesh_kill_request::Session,
    sesh_resize_request,
    seshd_client::SeshdClient,
    SeshInfo, SeshResizeRequest, SeshStartRequest, WinSize,
};

#[repr(u8)]
#[derive(Debug, Clone)]
enum ExitKind {
    Quit,
    Detach,
}

/// Formats the given input as green, then resets
macro_rules! success {
    ($($arg:tt)*) => {
        format!(
            "{}{}{}",
            color::Fg(color::Green),
            format!($($arg)*),
            color::Fg(color::Reset)
        )
    };
}

/// Formats the given input as red, then resets
macro_rules! error {
    ($($arg:expr),*) => {
        format!(
            "{}{}{}",
            color::Fg(color::Red),
            format!($($arg),*),
            color::Fg(color::Reset)
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

/// Responsible for executing a session, and managing its IO until it exits.
async fn exec_session(
    ctx: Ctx,
    pid: i32,
    socket: String,
    name: String,
    program: String,
) -> Result<ExitKind> {
    std::env::set_var("SESH_NAME", &name);
    std::io::stdout().write_all(format!("\x1B]0;{}\x07", "test").as_bytes())?;
    let mut tty_output = get_tty()
        .context("Failed to get tty")?
        .into_raw_mode()
        .context("Failed to set raw mode")?
        .into_alternate_screen()
        .context("Failed to enter alternate screen")?;

    // Set terminal title
    tty_output.write_all(format!("\x1B]0;{}\x07", program).as_bytes())?;

    let mut tty_input = tty_output.try_clone()?;

    let sock = PathBuf::from(&socket);
    let sock_dir = sock
        .parent()
        .ok_or(anyhow::anyhow!("Could not get runtime dir"))?;
    let client_server_sock = sock_dir.join(format!("client-{}.sock", pid));
    if client_server_sock.exists() {
        std::fs::remove_file(&client_server_sock).context(format!(
            "Failed to remove existing (server -> client) socket {}",
            &client_server_sock.display()
        ))?;
    }
    let uds = tokio::net::UnixListener::bind(&client_server_sock).context(format!(
        "Failed to bind listener to {}",
        &client_server_sock.display()
    ))?;
    let uds_stream = UnixListenerStream::new(uds);

    let (mut r_stream, mut w_stream) = UnixStream::connect(&socket)
        .await
        .context("Could not connect to socket stream")?
        .into_split();

    let r_handle = tokio::task::spawn({
        let mut tty_output = tty_output
            .try_clone()
            .context("Could not clone tty_output")?;
        let exit = ctx.exit.0.subscribe();
        async move {
            while exit.is_empty() {
                let mut packet = [0; 4096];

                let nbytes = r_stream.read(&mut packet).await?;
                if nbytes == 0 {
                    break;
                }
                let read = &packet[..nbytes];
                tty_output
                    .write_all(read)
                    .context("Could not write tty_output")?;
                tty_output.flush().context("Could not flush tty_output")?;
                // TODO: Use a less hacky method of reducing CPU usage
                tokio::time::sleep(tokio::time::Duration::from_nanos(200)).await;
            }
            Result::<_, anyhow::Error>::Ok(())
        }
    });
    let w_handle = tokio::task::spawn({
        let ctx = ctx.copy();
        let name = name.clone();
        async move {
            while ctx.exit.1.is_empty() {
                let mut packet = [0; 4096];

                let nbytes = tty_input
                    .read(&mut packet)
                    .context("Failed to read tty_input")?;
                if nbytes == 0 {
                    break;
                }
                let read = &packet[..nbytes];

                // Alt-\
                // TODO: Make this configurable

                if nbytes >= 2 && read[0] == 27 && read[1] == 92 {
                    detach_session(ctx, Some(SessionSelector::Name(name))).await?;
                    break;
                }

                w_stream
                    .write_all(read)
                    .await
                    .context("Failed to write to w_stream")?;
                w_stream.flush().await.context("Failed to flush w_stream")?;
                // TODO: Use a less hacky method of reducing CPU usage
                // tokio::time::sleep(tokio::time::Duration::from_nanos(20)).await;
            }
            Result::<_, anyhow::Error>::Ok(())
        }
    });
    let w_abort_handle = w_handle.abort_handle();

    tokio::task::spawn({
        let (exit_tx, mut exit_rx) = (ctx.exit.0.clone(), ctx.exit.0.subscribe());
        async move {
            RPCServer::builder()
                .add_service(SeshCliServer::new(SeshCliService { exit_tx }))
                .serve_with_incoming_shutdown(uds_stream, async move {
                    exit_rx.recv().await.ok();
                })
                .await?;
            w_abort_handle.abort();
            Result::<_, anyhow::Error>::Ok(())
        }
    });

    tokio::task::spawn({
        let name = name.clone();
        let mut ctx = ctx.copy();
        async move {
            let mut signal =
                signal(SignalKind::window_change()).context("Could not read SIGWINCH")?;
            while ctx.exit.0.is_empty() {
                signal.recv().await;
                let size = {
                    let s = termion::terminal_size().unwrap_or((80, 24));
                    WinSize {
                        rows: s.1 as u32,
                        cols: s.0 as u32,
                    }
                };
                ctx.client
                    .resize_session(SeshResizeRequest {
                        size: Some(size),
                        session: Some(sesh_resize_request::Session::Name(name.clone())),
                    })
                    .await
                    .context("Failed to resize")?;
            }
            Result::<_, anyhow::Error>::Ok(())
        }
    });

    let (exit_tx, exit_rx) = ctx.exit;
    let mut res_rx = exit_tx.subscribe();
    while exit_rx.is_empty() {
        unsafe {
            // This doesn't actually kill the process, it just checks if it exists
            if libc::kill(pid, 0) == -1 {
                // check errno
                // TODO: Figure out why this doesn't work on M1/M2 macs
                #[cfg(target_arch = "aarch64")]
                let errno = *libc::__error();
                #[cfg(not(target_arch = "aarch64"))]
                let errno = *libc::__errno_location();
                if errno == 3 {
                    //libc::ESRCH {
                    // process doesn't exist / has exited
                    exit_tx.send(ExitKind::Quit)?;
                    break;
                }
            }
        }
        // TODO: Use a less hacky method of reducing CPU usage
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    }

    // tokio::fs::remove_file(&client_server_sock).await?;
    // the write handle will block if it's not aborted
    w_handle.abort();
    // r_handle.await??;
    r_handle.abort();
    res_rx.recv().await.context("Could not receive exit event")
}

fn get_program(program: Option<String>) -> String {
    program.unwrap_or_else(|| std::env::var("SHELL").unwrap_or("bash".to_owned()))
}

/// Sends a start session request to the server, and handles the response
async fn start_session(
    mut ctx: Ctx,
    name: Option<String>,
    program: Option<String>,
    args: Vec<String>,
    attach: bool,
) -> anyhow::Result<Option<String>> {
    let program = get_program(program);
    let size = {
        let s = termion::terminal_size().unwrap_or((80, 24));
        WinSize {
            rows: s.1 as u32,
            cols: s.0 as u32,
        }
    };
    let req = tonic::Request::new(SeshStartRequest {
        name: name.unwrap_or_else(|| program.clone()),
        program,
        args,
        size: Some(size),
        pwd: std::env::current_dir()?.to_string_lossy().to_string(),
        env: std::env::vars()
            .map(|v| sesh_proto::Var {
                key: v.0,
                value: v.1,
            })
            .collect(),
    });

    let res = ctx
        .client
        .start_session(req)
        .await
        .map_err(|e| anyhow::anyhow!("Could not start session: {}", e))?
        .into_inner();
    if attach {
        match exec_session(ctx, res.pid, res.socket, res.name, res.program).await? {
            ExitKind::Quit => Ok(Some(success!("[exited]"))),
            ExitKind::Detach => Ok(Some(success!("[detached]"))),
        }
    } else {
        Ok(Some(success!("[started]")))
    }
}

/// Sends an attach session request to the server, and handles the response
async fn attach_session(
    mut ctx: Ctx,
    session: SessionSelector,
    create: bool,
) -> Result<Option<String>> {
    use sesh_proto::sesh_attach_request::Session::*;
    let session_resolved = match &session {
        SessionSelector::Id(id) => Id(*id as u64),
        SessionSelector::Name(name) => Name(name.clone()),
    };
    let size = {
        let s = termion::terminal_size().unwrap_or((80, 24));
        WinSize {
            rows: s.1 as u32,
            cols: s.0 as u32,
        }
    };
    let req = tonic::Request::new(sesh_proto::SeshAttachRequest {
        session: Some(session_resolved),
        size: Some(size),
    });
    let res = match ctx.client.attach_session(req).await {
        Ok(res) => res.into_inner(),
        Err(_) if create => return start_session(ctx, session.name(), None, vec![], true).await,
        Err(e) => return Err(anyhow::anyhow!("Session not found: {e}")),
    };

    match exec_session(ctx, res.pid, res.socket, res.name, res.program).await? {
        ExitKind::Quit => Ok(Some(success!("[exited]"))),
        ExitKind::Detach => Ok(Some(success!("[detached]"))),
    }
}

/// Sends a detach session request to the server, and handles the response
async fn detach_session(mut ctx: Ctx, session: Option<SessionSelector>) -> Result<Option<String>> {
    use sesh_proto::sesh_detach_request::Session::*;
    let session = match session {
        Some(SessionSelector::Id(id)) => Id(id as u64),
        Some(SessionSelector::Name(name)) => Name(name),
        None => {
            let Ok(current) = std::env::var("SESH_NAME") else {
                return Err(anyhow::anyhow!("No session name found in environment"));
            };
            Name(current)
        }
    };
    let request = tonic::Request::new(sesh_proto::SeshDetachRequest {
        session: Some(session),
    });
    let _response = ctx.client.detach_session(request).await?;
    ctx.exit.0.send(ExitKind::Detach)?;

    Ok(None)
}

/// Sends a list sessions request to the server, and handles the response
async fn kill_session(mut ctx: Ctx, session: SessionSelector) -> Result<Option<String>> {
    let request = tonic::Request::new(sesh_proto::SeshKillRequest {
        session: Some(match &session {
            SessionSelector::Id(id) => Session::Id(*id as u64),
            SessionSelector::Name(name) => Session::Name(name.clone()),
        }),
    });
    let response = ctx.client.kill_session(request).await?;
    if response.into_inner().success {
        Ok(Some(success!("[killed {}]", session)))
    } else {
        Err(anyhow::anyhow!("{}", error!("Could not kill process")))
    }
}

// TODO: Make these configurable
/// Active session icon
static ACTIVE_ICON: char = '⯌';
/// Bullet icon
static BULLET_ICON: char = '❒';

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

#[derive(serde::Serialize, serde::Deserialize)]
struct SeshInfoSer {
    index: usize,
    name: String,
    program: String,
    socket: String,
    connected: bool,
    start_time: i64,
    attach_time: i64,
}

/// Sends a list sessions request to the server, and handles the response
async fn list_sessions(mut ctx: Ctx, table: bool, json: bool) -> Result<Option<String>> {
    let request = tonic::Request::new(sesh_proto::SeshListRequest {});
    let response = ctx.client.list_sessions(request).await?.into_inner();
    let sessions = &response.sessions;

    match ListMode::new(table, json) {
        ListMode::List => {
            let mut res = String::new();
            for (i, session) in sessions.iter().enumerate() {
                if i > 0 {
                    res += "\n";
                }
                let bullet = if session.connected {
                    success!("{}{}", Bold, BULLET_ICON)
                } else {
                    format!("{}{}", Bold, BULLET_ICON)
                };
                res += &format!(
                    "{bullet} {col}{id}{reset} \u{2218} {name} \u{2218} {program}{reset_attr}",
                    id = session.id,
                    name = session.name,
                    program = session.program.split('/').last().unwrap_or(""),
                    col = Fg(color::LightBlue),
                    reset = Fg(color::Reset),
                    reset_attr = termion::style::Reset
                );
            }
            Ok(Some(res))
        }
        ListMode::Table => {
            let mut table = Table::new();
            table.set_format(
                FormatBuilder::new()
                    .column_separator('│')
                    .borders('│')
                    .separator(LinePosition::Top, LineSeparator::new('─', '┬', '╭', '╮'))
                    // -------
                    .separator(LinePosition::Intern, LineSeparator::new('─', '┼', '├', '┤'))
                    // -------
                    .separator(LinePosition::Bottom, LineSeparator::new('─', '┴', '╰', '╯'))
                    .padding(1, 1)
                    .build(),
            );
            // const SOCKET_ICON: char = '';
            //
            table.set_titles(row![
                icon_title('', "Id", Fg(color::LightRed)),
                icon_title('', "Name", Fg(color::LightBlue)),
                icon_title('', "Started", Fg(color::LightYellow)),
                icon_title('', "Attached", Fg(color::LightGreen)),
                icon_title('', "Program", Fg(color::LightCyan)),
                icon_title('', "PID", Fg(color::LightMagenta))
            ]);
            sessions.iter().for_each(|s: &SeshInfo| {
                let connected = if s.connected {
                    success!(" {}{}", Fg(color::LightGreen), ACTIVE_ICON)
                } else {
                    "".to_owned()
                };
                let s_time = Local.timestamp_millis_opt(s.start_time).unwrap();
                table.add_row(row![
                    format!(
                        "{col}{}{reset}",
                        s.id,
                        col = Fg(color::LightBlue),
                        reset = Fg(color::Reset)
                    ),
                    format!("{}{}{reset}", s.name, connected, reset = Fg(color::Reset)),
                    s_time.format("%m/%d/%g \u{2218} %I:%M%P"),
                    if s.attach_time > 0 {
                        match Local.timestamp_millis_opt(s.attach_time) {
                            chrono::LocalResult::None => "Unknown".to_owned(),
                            chrono::LocalResult::Single(a_time)
                            | chrono::LocalResult::Ambiguous(a_time, _) => {
                                a_time.format("%m/%d/%g \u{2218} %I:%M%P").to_string()
                            }
                        }
                    } else {
                        "Never".to_owned()
                    },
                    // s.socket
                    s.program,
                    s.pid
                ]);
            });
            let mut rendered = Cursor::new(Vec::new());
            table.print(&mut rendered)?;
            let s = String::from_utf8(rendered.into_inner())?;
            Ok(Some(s))
        }
        ListMode::Json => {
            let sessions = sessions
                .iter()
                .map(|s| SeshInfoSer {
                    index: s.id as usize,
                    name: s.name.clone(),
                    program: s.program.clone(),
                    socket: s.socket.clone(),
                    connected: s.connected,
                    start_time: s.start_time,
                    attach_time: s.attach_time,
                })
                .collect::<Vec<_>>();
            let json = serde_json::to_string_pretty(&sessions)?;
            Ok(Some(json))
        }
    }
}

/// Sends a shutdown request to the server
async fn shutdown_server(mut ctx: Ctx) -> Result<Option<String>> {
    let request = tonic::Request::new(sesh_proto::ShutdownServerRequest {});
    let response = ctx.client.shutdown_server(request).await?;
    Ok(Some(if response.into_inner().success {
        success!("[shutdown]")
    } else {
        return Err(anyhow::anyhow!("Failed to shutdown server"));
    }))
}

/// Wraps the `list_sessions` and `attach_session` requests to allow fuzzy searching over sessions
async fn select_session(mut ctx: Ctx) -> Result<Option<String>> {
    let request = tonic::Request::new(sesh_proto::SeshListRequest {});
    let response = ctx.client.list_sessions(request).await?.into_inner();
    let sessions = response
        .sessions
        .into_iter()
        .map(|s| s.name)
        .collect::<Vec<_>>();

    let Ok(Some(select)) = dialoguer::FuzzySelect::with_theme(&theme::ColorfulTheme::default())
        .items(sessions.as_slice())
        .default(0)
        .report(true)
        .with_prompt("Session")
        .interact_opt() else {
            return Ok(Some(success!("[cancelled]")));
        };

    let Some(name) = sessions.get(select) else {
        return Err(anyhow::anyhow!("Invalid selection"));
    };

    attach_session(ctx, SessionSelector::Name(name.clone()), false).await
}

async fn resume_session(mut ctx: Ctx, create: bool) -> Result<Option<String>> {
    let request = tonic::Request::new(sesh_proto::SeshListRequest {});
    let mut sessions = ctx
        .client
        .list_sessions(request)
        .await?
        .into_inner()
        .sessions;
    sessions.retain(|s| !s.connected);
    sessions.sort_by(|a, b| a.attach_time.cmp(&b.attach_time));
    let session = sessions.into_iter().last();
    match session {
        Some(session) => attach_session(ctx, SessionSelector::Name(session.name), false).await,
        None if create => start_session(ctx, None, None, vec![], true).await,
        None => Ok(Some(error!("[no sessions to resume]"))),
    }
}

/// Initializes the Tonic client with a UnixStream from the provided socket path
/// Sets up exit broadcast / mpmc channel
struct Ctx {
    client: SeshdClient<Channel>,
    exit: (broadcast::Sender<ExitKind>, broadcast::Receiver<ExitKind>),
}

impl Ctx {
    pub async fn init(socket: PathBuf) -> Result<Self> {
        if !socket.exists() {
            return Err(anyhow::anyhow!(
                "Server socket not found at {}",
                socket.display()
            ));
        }

        // Create a channel to the server socket
        let channel = Endpoint::try_from("http://[::]:50051")?
            .connect_with_connector(service_fn(move |_: Uri| {
                // Connect to a Uds socket
                UnixStream::connect(socket.clone())
            }))
            .await?;

        let client = SeshdClient::new(channel);

        let (tx, rx) = broadcast::channel(1);

        tokio::task::spawn({
            let tx = tx.clone();
            async move {
                let mut quit = unix::signal(SignalKind::quit())?;
                let mut int = unix::signal(SignalKind::interrupt())?;
                let mut term = unix::signal(SignalKind::terminate())?;
                let mut alarm = unix::signal(SignalKind::alarm())?;
                select! {
                    _ = quit.recv() => (),
                    _ = int.recv() => (),
                    _ = term.recv() => (),
                    _ = alarm.recv() => ()
                }
                tx.send(ExitKind::Quit)?;
                Result::<(), anyhow::Error>::Ok(())
            }
        });

        Ok(Ctx {
            client,
            exit: (tx, rx),
        })
    }

    fn copy(&self) -> Self {
        Ctx {
            client: self.client.clone(),
            exit: (self.exit.0.clone(), self.exit.0.subscribe()),
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
        } => start_session(ctx, name, program, args, !detached).await,
        Command::Resume { create } => resume_session(ctx, create).await,
        Command::Attach { session, create } => attach_session(ctx, session, create).await,
        Command::Kill { session } => kill_session(ctx, session).await,
        Command::Detach { session } => detach_session(ctx, session).await,
        Command::Select => select_session(ctx).await,
        Command::List { info, json } => list_sessions(ctx, info, json).await,
        Command::Shutdown => shutdown_server(ctx).await,
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
