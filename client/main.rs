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
//! **Usage:** `sesh [OPTIONS] [PROGRAM] [ARGS]... [COMMAND]`
//!
//! ###### **Subcommands:**
//!
//! * `resume` — Resume the last used session [alias: r]
//! * `start` — Start a new session, optionally specifying a name [alias: s]
//! * `attach` — Attach to a session [alias: a]
//! * `select` — Fuzzy select a session to attach to [alias: f]
//! * `detach` — Detach the current session or the specified session [alias: d]
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
//! **Usage:** `sesh resume`
//!
//!
//!
//! ## `sesh start`
//!
//! Start a new session, optionally specifying a name [alias: s]
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
//! **Usage:** `sesh attach <SESSION>`
//!
//! ###### **Arguments:**
//!
//! * `<SESSION>` — Id or name of session
//!
//!
//!
//! ## `sesh select`
//!
//! Fuzzy select a session to attach to [alias: f]
//!
//! **Usage:** `sesh select`
//!
//!
//!
//! ## `sesh detach`
//!
//! Detach the current session or the specified session [alias: d]
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
//!
//!
//!
//! <hr/>

use std::{
    io::{Cursor, Read, Write},
    path::PathBuf,
    process::ExitCode,
    sync::atomic::{AtomicBool, Ordering},
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
    signal::unix::{signal, SignalKind},
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

// TODO: Use message passing instead
static mut EXIT: AtomicBool = AtomicBool::new(false);
static mut DETACHED: AtomicBool = AtomicBool::new(false);

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
struct SeshCliService;

#[tonic::async_trait]
impl SeshCli for SeshCliService {
    /// Server -> Client request to detach a session
    async fn detach(
        &self,
        _: tonic::Request<sesh_proto::ClientDetachRequest>,
    ) -> std::result::Result<tonic::Response<sesh_proto::ClientDetachResponse>, tonic::Status> {
        unsafe {
            EXIT.store(true, Ordering::Relaxed);
            DETACHED.store(true, Ordering::Relaxed);
        }
        Ok(tonic::Response::new(sesh_proto::ClientDetachResponse {}))
    }
}

/// Responsible for executing a session, and managing its IO until it exits.
async fn exec_session(
    client: SeshdClient<Channel>,
    pid: i32,
    socket: String,
    name: String,
) -> Result<()> {
    std::env::set_var("SESH_NAME", &name);
    let tty_output = get_tty()
        .context("Failed to get tty")?
        .into_raw_mode()
        .context("Failed to set raw mode")?
        .into_alternate_screen()
        .context("Failed to enter alternate screen")?;
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
        async move {
            while !unsafe { EXIT.load(Ordering::Relaxed) } {
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
        let client = client.clone();
        let name = name.clone();
        async move {
            while !unsafe { EXIT.load(Ordering::Relaxed) } {
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
                    detach_session(client, Some(SessionSelector::Name(name))).await?;
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
        async move {
            RPCServer::builder()
                .add_service(SeshCliServer::new(SeshCliService))
                .serve_with_incoming_shutdown(uds_stream, async move {
                    while !unsafe { EXIT.load(Ordering::Relaxed) } {
                        tokio::time::sleep(tokio::time::Duration::from_millis(150)).await;
                    }
                })
                .await?;
            w_abort_handle.abort();
            Result::<_, anyhow::Error>::Ok(())
        }
    });

    tokio::task::spawn({
        let name = name.clone();
        let mut client = client.clone();
        async move {
            let mut signal =
                signal(SignalKind::window_change()).context("Could not read SIGWINCH")?;
            while !unsafe { EXIT.load(Ordering::Relaxed) } {
                signal.recv().await;
                let size = {
                    let s = termion::terminal_size().unwrap_or((80, 24));
                    WinSize {
                        rows: s.1 as u32,
                        cols: s.0 as u32,
                    }
                };
                client
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

    while !unsafe { EXIT.load(Ordering::Relaxed) } {
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
                    EXIT.store(true, Ordering::Relaxed);
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
    Ok(())
}

fn get_program(program: Option<String>) -> String {
    program.unwrap_or_else(|| std::env::var("SHELL").unwrap_or("bash".to_owned()))
}

/// Sends a start session request to the server, and handles the response
async fn start_session(
    mut client: SeshdClient<Channel>,
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
    });
    let res = client
        .start_session(req)
        .await
        .map_err(|e| anyhow::anyhow!("Could not start session: {}", e))?
        .into_inner();
    if attach {
        exec_session(client, res.pid, res.socket, res.name).await?;
    }
    if unsafe { DETACHED.load(Ordering::Relaxed) } {
        Ok(Some(success!("[detached]")))
    } else if !attach {
        Ok(Some(success!("[started]")))
    } else {
        Ok(Some(success!("[exited]")))
    }
}

/// Sends an attach session request to the server, and handles the response
async fn attach_session(
    mut client: SeshdClient<Channel>,
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
    let res = match client.attach_session(req).await {
        Ok(res) => res.into_inner(),
        Err(_) if create => return start_session(client, session.name(), None, vec![], true).await,
        Err(e) => return Err(anyhow::anyhow!("Session not found: {e}")),
    };

    // match session {
    //     Some(session) => attach_session(client, SessionSelector::Name(session.name)).await,
    //     None if create => start_session(client, None, None, vec![], true).await,
    //     None => Ok(Some(error!("[no sessions to resume]"))),
    // }
    // .context("Could not attach session")?
    // .into_inner();
    exec_session(client, res.pid, res.socket, res.name).await?;
    if unsafe { DETACHED.load(Ordering::Relaxed) } {
        Ok(Some(success!("[detached]")))
    } else {
        Ok(Some(success!("[exited]")))
    }
}

/// Sends a detach session request to the server, and handles the response
async fn detach_session(
    mut client: SeshdClient<Channel>,
    session: Option<SessionSelector>,
) -> Result<Option<String>> {
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
    let _response = client.detach_session(request).await?;
    unsafe {
        EXIT.store(true, Ordering::Relaxed);
        DETACHED.store(true, Ordering::Relaxed);
    }

    Ok(None)
}

/// Sends a list sessions request to the server, and handles the response
async fn kill_session(
    mut client: SeshdClient<Channel>,
    session: SessionSelector,
) -> Result<Option<String>> {
    let request = tonic::Request::new(sesh_proto::SeshKillRequest {
        session: Some(match &session {
            SessionSelector::Id(id) => Session::Id(*id as u64),
            SessionSelector::Name(name) => Session::Name(name.clone()),
        }),
    });
    let response = client.kill_session(request).await?;
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

/// Sends a list sessions request to the server, and handles the response
async fn list_sessions(mut client: SeshdClient<Channel>, table: bool) -> Result<Option<String>> {
    let request = tonic::Request::new(sesh_proto::SeshListRequest {});
    let response = client.list_sessions(request).await?.into_inner();
    let sessions = &response.sessions;

    if table {
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
            icon_title('', "Socket", Fg(color::LightCyan)),
        ]);
        sessions.iter().for_each(|s: &SeshInfo| {
            // let bullet = if s.connected {
            //     success!("{}{}", Bold, bullets[0])
            // } else {
            //     format!("{}{}", Bold, bullets[0])
            // };
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
                s.socket
            ]);
        });
        let mut rendered = Cursor::new(Vec::new());
        table.print(&mut rendered)?;
        let s = String::from_utf8(rendered.into_inner())?;
        Ok(Some(s))
    } else {
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
                "{} {col}{}{reset} \u{2218} {}",
                bullet,
                session.id,
                session.name,
                col = Fg(color::LightBlue),
                reset = Fg(color::Reset)
            );
        }
        Ok(Some(res))
    }
}

/// Initializes the Tonic client with a UnixStream from the provided socket path
async fn init_client(sock_path: PathBuf) -> Result<SeshdClient<Channel>> {
    if !sock_path.exists() {
        return Err(anyhow::anyhow!(
            "Server socket not found at {}",
            sock_path.display()
        ));
    }

    // Create a channel to the server socket
    let channel = Endpoint::try_from("http://[::]:50051")?
        .connect_with_connector(service_fn(move |_: Uri| {
            // Connect to a Uds socket
            UnixStream::connect(sock_path.clone())
        }))
        .await?;

    Ok(SeshdClient::new(channel))
}

/// Sends a shutdown request to the server
async fn shutdown_server(mut client: SeshdClient<Channel>) -> Result<Option<String>> {
    let request = tonic::Request::new(sesh_proto::ShutdownServerRequest {});
    let response = client.shutdown_server(request).await?;
    Ok(Some(if response.into_inner().success {
        success!("[shutdown]")
    } else {
        return Err(anyhow::anyhow!("Failed to shutdown server"));
    }))
}

/// Wraps the `list_sessions` and `attach_session` requests to allow fuzzy searching over sessions
async fn select_session(mut client: SeshdClient<Channel>) -> Result<Option<String>> {
    let request = tonic::Request::new(sesh_proto::SeshListRequest {});
    let response = client.list_sessions(request).await?.into_inner();
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

    attach_session(client, SessionSelector::Name(name.clone()), false).await
}

async fn resume_session(mut client: SeshdClient<Channel>, create: bool) -> Result<Option<String>> {
    let request = tonic::Request::new(sesh_proto::SeshListRequest {});
    let mut sessions = client.list_sessions(request).await?.into_inner().sessions;
    sessions.retain(|s| !s.connected);
    sessions.sort_by(|a, b| a.attach_time.cmp(&b.attach_time));
    let session = sessions.into_iter().last();
    match session {
        Some(session) => attach_session(client, SessionSelector::Name(session.name), false).await,
        None if create => start_session(client, None, None, vec![], true).await,
        None => Ok(Some(error!("[no sessions to resume]"))),
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    let Ok(_) = ctrlc::set_handler(move || unsafe {
        EXIT.store(true, Ordering::Relaxed);
    }) else {
        eprintln!("{}", error!("[failed to set ctrl-c handler]"));
        return ExitCode::FAILURE;
    };
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
            while !server_sock.exists() {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }
    }

    let Ok(client) = init_client(server_sock).await else {
        eprintln!("{}", error!("[failed to connect to server]"));
        return ExitCode::FAILURE;
    };

    let message = match cmd {
        Command::Start {
            name,
            program,
            args,
            detached,
        } => start_session(client, name, program, args, !detached).await,
        Command::Resume { create } => resume_session(client, create).await,
        Command::Attach { session, create } => attach_session(client, session, create).await,
        Command::Kill { session } => kill_session(client, session).await,
        Command::Detach { session } => detach_session(client, session).await,
        Command::Select => select_session(client).await,
        Command::List { info } => list_sessions(client, info).await,
        Command::Shutdown => shutdown_server(client).await,
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
