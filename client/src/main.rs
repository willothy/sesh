use std::{
    fmt::Display,
    io::{Read, Write},
    path::PathBuf,
    process::ExitCode,
    str::FromStr,
    sync::atomic::{AtomicBool, Ordering},
};

use anyhow::Result;
use clap::{Parser, Subcommand};
use sesh_shared::{pty::Pty, term::Size};
use termion::{color, get_tty, raw::IntoRawMode, screen::IntoAlternateScreen};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
    signal::unix::{signal, SignalKind},
};
use tokio_stream::wrappers::UnixListenerStream;
use tonic::transport::{Channel, Endpoint, Uri};
use tower::service_fn;

use sesh_proto::{
    sesh_cli_server::{SeshCli, SeshCliServer},
    sesh_kill_request::Session,
    sesh_resize_request,
    seshd_client::SeshdClient,
    SeshResizeRequest, SeshStartRequest, WinSize,
};

static mut EXIT: AtomicBool = AtomicBool::new(false);
static mut DETACHED: AtomicBool = AtomicBool::new(false);

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

#[derive(Debug, clap::Parser)]
#[clap(
    name = "sesh",
    version = "0.1.0",
    author = "Will Hopkins <willothyh@gmail.com>"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    #[command(alias = "s")]
    /// Start a new session, optionally specifying a name [alias: s]
    Start {
        #[arg(short, long)]
        name: Option<String>,
        program: Option<String>,
        args: Vec<String>,
        #[arg(short, long)]
        detached: bool,
    },
    #[command(alias = "a")]
    /// Attach to a session [alias: a]
    Attach {
        /// Id or name of session
        session: SessionSelector,
    },
    /// Detach a session remotely [alias: d]
    /// Detaches the current session, or the one specified
    #[command(alias = "d")]
    Detach {
        /// Id or name of session
        session: Option<SessionSelector>,
    },
    #[command(alias = "k")]
    /// Kill a session [alias: k]
    Kill {
        /// Id or name of session
        session: SessionSelector,
    },
    /// List sessions [alias: ls]
    #[command(alias = "ls")]
    List,
    /// Shutdown the server (kill all sessions)
    Shutdown,
}

#[derive(Debug, Clone)]
enum SessionSelector {
    Id(usize),
    Name(String),
}

impl Display for SessionSelector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionSelector::Id(id) => write!(f, "{}", id),
            SessionSelector::Name(name) => write!(f, "{}", name),
        }
    }
}

impl FromStr for SessionSelector {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Ok(id) = s.parse::<usize>() {
            Ok(SessionSelector::Id(id))
        } else {
            Ok(SessionSelector::Name(s.to_owned()))
        }
    }
}

#[derive(Clone)]
struct SeshCliService;

#[tonic::async_trait]
impl SeshCli for SeshCliService {
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

use tonic::transport::Server as RPCServer;
async fn exec_session(
    client: SeshdClient<Channel>,
    pid: i32,
    socket: String,
    name: String,
) -> Result<()> {
    std::env::set_var("SESH_NAME", &name);
    let mut tty_output = get_tty()?.into_alternate_screen()?.into_raw_mode()?;
    tty_output.activate_raw_mode()?;

    let sock = PathBuf::from(&socket);
    let sock_dir = sock
        .parent()
        .ok_or(anyhow::anyhow!("Could not get runtime dir"))?;
    let client_server_sock = sock_dir.join(format!("client-{}.sock", pid));
    let uds = tokio::net::UnixListener::bind(&client_server_sock)?;
    let uds_stream = UnixListenerStream::new(uds);

    tokio::task::spawn(async move {
        RPCServer::builder()
            .add_service(SeshCliServer::new(SeshCliService))
            .serve_with_incoming_shutdown(uds_stream, async move {
                while unsafe { EXIT.load(Ordering::Relaxed) } == false {
                    tokio::time::sleep(tokio::time::Duration::from_millis(150)).await;
                }
            })
            .await?;
        Result::<_, anyhow::Error>::Ok(())
    });

    let mut tty_input = tty_output.try_clone()?;

    let (mut r_stream, mut w_stream) = UnixStream::connect(&socket).await?.into_split();

    let r_handle = tokio::task::spawn(async move {
        while unsafe { EXIT.load(Ordering::Relaxed) } == false {
            let mut packet = [0; 4096];

            let nbytes = r_stream.read(&mut packet).await?;
            if nbytes == 0 {
                break;
            }
            let read = &packet[..nbytes];
            tty_output.write_all(&read)?;
            tty_output.flush()?;
            // TODO: Use a less hacky method of reducing CPU usage
            tokio::time::sleep(tokio::time::Duration::from_nanos(200)).await;
        }
        Result::<_, anyhow::Error>::Ok(())
    });
    let w_handle = tokio::task::spawn({
        let client = client.clone();
        let name = name.clone();
        async move {
            while unsafe { EXIT.load(Ordering::Relaxed) } == false {
                let mut packet = [0; 4096];

                let nbytes = tty_input.read(&mut packet)?;
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

                w_stream.write_all(&read).await?;
                w_stream.flush().await?;
                // TODO: Use a less hacky method of reducing CPU usage
                // tokio::time::sleep(tokio::time::Duration::from_nanos(20)).await;
            }
            Result::<_, anyhow::Error>::Ok(())
        }
    });

    tokio::task::spawn({
        let name = name.clone();
        let mut client = client.clone();
        async move {
            while unsafe { EXIT.load(Ordering::Relaxed) } == false {
                signal(SignalKind::window_change())?.recv().await;
                let size = {
                    let s = termion::terminal_size()?;
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
                    .await?;
            }
            Result::<_, anyhow::Error>::Ok(())
        }
    });

    while unsafe { EXIT.load(Ordering::Relaxed) } == false {
        unsafe {
            // This doesn't actually kill the process, it just checks if it exists
            if libc::kill(pid, 0) == -1 {
                // check errno
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

    tokio::fs::remove_file(&client_server_sock).await?;
    // the write handle will block if it's not aborted
    w_handle.abort();
    r_handle.await??;
    Ok(())
}

async fn start_session(
    mut client: SeshdClient<Channel>,
    name: Option<String>,
    program: Option<String>,
    args: Vec<String>,
    attach: bool,
) -> anyhow::Result<Option<String>> {
    let program = program.unwrap_or_else(|| std::env::var("SHELL").unwrap_or("sh".to_owned()));
    let size = {
        let s = termion::terminal_size()?;
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
    });
    let res = client.start_session(req).await?.into_inner();
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

async fn attach_session(
    mut client: SeshdClient<Channel>,
    session: SessionSelector,
) -> Result<Option<String>> {
    use sesh_proto::sesh_attach_request::Session::*;
    let session = match session {
        SessionSelector::Id(id) => Id(id as u64),
        SessionSelector::Name(name) => Name(name),
    };
    let size = {
        let s = termion::terminal_size()?;
        WinSize {
            rows: s.1 as u32,
            cols: s.0 as u32,
        }
    };
    let req = tonic::Request::new(sesh_proto::SeshAttachRequest {
        session: Some(session),
        size: Some(size),
    });
    let res = client.attach_session(req).await?.into_inner();
    exec_session(client, res.pid, res.socket, res.name).await?;
    if unsafe { DETACHED.load(Ordering::Relaxed) } {
        Ok(Some(success!("[detached]")))
    } else {
        Ok(Some(success!("[exited]")))
    }
}

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
        return Ok(Some(format!("[killed {}]", session)));
    } else {
        return Err(anyhow::anyhow!("{}", error!("Could not kill process")));
    }
}

async fn list_sessions(mut client: SeshdClient<Channel>) -> Result<Option<String>> {
    let request = tonic::Request::new(sesh_proto::SeshListRequest {});
    let response = client.list_sessions(request).await?.into_inner();
    let sessions = &response.sessions;

    let mut res = String::new();
    for (i, session) in sessions.iter().enumerate() {
        if i > 0 {
            res += "\n";
        }
        res += &format!("◦ {}: {}", session.id, session.name);
    }
    Ok(Some(res))
}

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

async fn shutdown_server(mut client: SeshdClient<Channel>) -> Result<Option<String>> {
    let request = tonic::Request::new(sesh_proto::ShutdownServerRequest {});
    let response = client.shutdown_server(request).await?;
    Ok(Some(if response.into_inner().success {
        success!("[shutdown]")
    } else {
        return Err(anyhow::anyhow!("Failed to shutdown server"));
    }))
}

#[tokio::main]
async fn main() -> ExitCode {
    let Ok(_) = ctrlc::set_handler(move || unsafe {
        EXIT.store(true, Ordering::Relaxed);
    }) else {
        eprintln!("{}", error!("[failed to set ctrl-c handler]"));
        return ExitCode::FAILURE;
    };
    let args = Cli::parse();

    let rt = dirs::runtime_dir()
        .unwrap_or(PathBuf::from("/tmp/"))
        .join("sesh/");
    let server_sock = rt.join("server.sock");

    let cmd = match args.command {
        Some(cmd) => cmd,
        None => Command::List,
    };
    if !server_sock.exists() {
        if matches!(cmd, Command::Shutdown)
            || matches!(cmd, Command::List)
            || matches!(cmd, Command::Kill { .. })
        {
            println!("{}", error!("[not running]"));
            return ExitCode::FAILURE;
        } else {
            let Ok(size) = Size::term_size() else {
                return ExitCode::FAILURE;
            };
            let Ok(_) = Pty::new(&std::env::var("SESHD_PATH").unwrap_or("seshd".to_owned()))
                .daemonize()
                .env("RUST_LOG", "INFO")
                .spawn(&size) else {
                    return ExitCode::FAILURE;
                };
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
        Command::Attach { session } => attach_session(client, session).await,
        Command::Kill { session } => kill_session(client, session).await,
        Command::Detach { session } => detach_session(client, session).await,
        Command::List => list_sessions(client).await,
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

    // TODO: exit more cleanly
    std::process::exit(0);
}
