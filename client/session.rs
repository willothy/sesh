use std::io::Cursor;
use std::path::PathBuf;

use anyhow::{Context, Result};
use chrono::{Local, TimeZone};
use dialoguer::theme;
use prettytable::format::{FormatBuilder, LinePosition, LineSeparator};
use prettytable::{row, Table};
use sesh_cli::SessionSelector;
use sesh_proto::seshd_client::SeshdClient;
use sesh_proto::SeshInfo;
use sesh_proto::{
    sesh_cli_server::SeshCliServer, sesh_kill_request::Session, sesh_resize_request,
    SeshResizeRequest, SeshStartRequest, WinSize,
};
use termion::color::{self, Fg};
use termion::{raw::IntoRawMode, screen::IntoAlternateScreen};
use tokio::sync::broadcast;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
    signal::unix::{signal, SignalKind},
};
use tokio_stream::wrappers::UnixListenerStream;
use tonic::transport::{Channel, Endpoint, Server as RPCServer, Uri};
use tower::service_fn;

use crate::{error, get_program, icon_title, success, ExitKind, ListMode, SeshCliService};

// TODO: Make these configurable
/// Active session icon
static ACTIVE_ICON: char = '⯌';
/// Bullet icon
static BULLET_ICON: char = '❒';

/// Initializes the Tonic client with a UnixStream from the provided socket path
/// Sets up exit broadcast / mpmc channel
pub struct Ctx {
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

        Ok(Ctx {
            client,
            exit: (tx, rx),
        })
    }
}

impl Clone for Ctx {
    fn clone(&self) -> Self {
        Ctx {
            client: self.client.clone(),
            exit: (self.exit.0.clone(), self.exit.0.subscribe()),
        }
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
    // NOTE: This is used to set raw mode and alternate screen while
    // still using tokio's async stdout.
    let _raw = std::io::stdout()
        .into_raw_mode()
        .context("Failed to set raw mode")?
        .into_alternate_screen()
        .context("Failed to enter alternate screen")?;

    let mut output = tokio::io::stdout();

    // Set terminal title
    output
        .write_all(format!("\x1B]0;{}\x07", program).as_bytes())
        .await?;

    let sock = PathBuf::from(&socket);
    let sock_dir = sock
        .parent()
        .ok_or(anyhow::anyhow!("Could not get runtime dir"))?;
    let client_server_sock = sock_dir.join(format!("client-{}.sock", pid));
    if client_server_sock.exists() {
        tokio::fs::remove_file(&client_server_sock)
            .await
            .context(format!(
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

    // Reads process output from the server and writes it to the terminal
    let mut r_handle = tokio::task::spawn({
        let exit = ctx.exit.0.subscribe();
        async move {
            let mut packet = [0; 4096];
            while exit.is_empty() {
                let bytes = r_stream.read(&mut packet).await?;
                if bytes == 0 {
                    break;
                }
                output
                    .write_all(&packet[..bytes])
                    .await
                    .context("Could not write tty_output")?;
                output.flush().await.context("Could not flush tty_output")?;
            }
            Result::<_, anyhow::Error>::Ok(())
        }
    });

    // Reads terminal input and sends it to the server to be handled by the process.
    let mut w_handle = tokio::task::spawn({
        let ctx = ctx.clone();
        let name = name.clone();
        async move {
            let mut input = tokio::io::stdin();
            while ctx.exit.1.is_empty() {
                let mut packet = [0; 4096];

                let nbytes = input
                    .read(&mut packet)
                    .await
                    .context("Failed to read tty_input")?;
                if nbytes == 0 {
                    break;
                }
                let read = &packet[..nbytes];

                // Alt-\
                // TODO: Make this configurable
                if nbytes >= 2 && read[0] == 27 && read[1] == 92 {
                    detach(ctx, Some(SessionSelector::Name(name))).await?;
                    break;
                }

                w_stream
                    .write_all(read)
                    .await
                    .context("Failed to write to w_stream")?;
                w_stream.flush().await.context("Failed to flush w_stream")?;
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
        let mut ctx = ctx.clone();
        async move {
            let mut signal = signal(SignalKind::window_change())?;
            loop {
                tokio::select! {
                    _ = ctx.exit.1.recv() => break,
                    _ = signal.recv() => {
                        let size = {
                            let s = termion::terminal_size().unwrap_or((80, 24));
                            WinSize {
                                rows: s.1 as u32,
                                cols: s.0 as u32,
                            }
                        };
                        ctx.client.resize_session(SeshResizeRequest {
                            size: Some(size),
                            session: Some(sesh_resize_request::Session::Name(name.clone())),
                        }).await.context("Failed to resize")?;
                    }
                }
            }
            Result::<_, anyhow::Error>::Ok(())
        }
    });

    let mut exit_rx = ctx.exit.1;
    let mut quit = signal(SignalKind::quit())?;
    let mut interrupt = signal(SignalKind::interrupt())?;
    let mut terminate = signal(SignalKind::terminate())?;
    let mut alarm = signal(SignalKind::alarm())?;
    let exit = tokio::select! {
        kind = exit_rx.recv() => kind.unwrap_or(ExitKind::Quit),
        _ = quit.recv() => ExitKind::Quit,
        _ = interrupt.recv() => ExitKind::Quit,
        _ = terminate.recv() => ExitKind::Quit,
        _ = alarm.recv() => ExitKind::Quit,
        _ = &mut r_handle => ExitKind::Quit,
        _ = &mut w_handle => ExitKind::Quit,
    };

    tokio::fs::remove_file(&client_server_sock).await.ok();
    // the write handle will block if it's not aborted
    w_handle.abort();
    r_handle.abort();
    Ok(exit)
}

/// Sends an attach session request to the server, and handles the response
pub async fn attach(
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
        Err(_) if create => return start(ctx, session.name(), None, vec![], true).await,
        Err(e) => return Err(anyhow::anyhow!("Session not found: {e}")),
    };

    match exec_session(ctx, res.pid, res.socket, res.name, res.program).await? {
        ExitKind::Quit => Ok(Some(success!("[exited]"))),
        ExitKind::Detach => Ok(Some(success!("[detached]"))),
    }
}

/// Sends a detach session request to the server, and handles the response
pub async fn detach(mut ctx: Ctx, session: Option<SessionSelector>) -> Result<Option<String>> {
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
pub async fn kill(mut ctx: Ctx, session: SessionSelector) -> Result<Option<String>> {
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

/// Sends a start session request to the server, and handles the response
pub async fn start(
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

/// Wraps the `list_sessions` and `attach_session` requests to allow fuzzy searching over sessions
pub async fn select(mut ctx: Ctx) -> Result<Option<String>> {
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
        .interact_opt()
    else {
        return Ok(Some(success!("[cancelled]")));
    };

    let Some(name) = sessions.get(select) else {
        return Err(anyhow::anyhow!("Invalid selection"));
    };

    attach(ctx, SessionSelector::Name(name.clone()), false).await
}

pub async fn resume(mut ctx: Ctx, create: bool) -> Result<Option<String>> {
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
        Some(session) => attach(ctx, SessionSelector::Name(session.name), false).await,
        None if create => start(ctx, None, None, vec![], true).await,
        None => Ok(Some(error!("[no sessions to resume]"))),
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
pub async fn list(mut ctx: Ctx, table: bool, json: bool) -> Result<Option<String>> {
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
                    success!("{}{}", termion::style::Bold, BULLET_ICON)
                } else {
                    format!("{}{}", termion::style::Bold, BULLET_ICON)
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
                    .separator(LinePosition::Intern, LineSeparator::new('─', '┼', '├', '┤'))
                    .separator(LinePosition::Bottom, LineSeparator::new('─', '┴', '╰', '╯'))
                    .padding(1, 1)
                    .build(),
            );
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
pub async fn shutdown(mut ctx: Ctx) -> Result<Option<String>> {
    let request = tonic::Request::new(sesh_proto::ShutdownServerRequest {});
    let response = ctx.client.shutdown_server(request).await?;
    Ok(Some(if response.into_inner().success {
        success!("[shutdown]")
    } else {
        return Err(anyhow::anyhow!("Failed to shutdown server"));
    }))
}
