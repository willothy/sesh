use std::{
    io::{Read, Write},
    path::PathBuf,
    str::FromStr,
    sync::atomic::{AtomicBool, Ordering},
};

use anyhow::Result;
use clap::{Parser, Subcommand};
use sesh_shared::{pty::Pty, term::Size};
use termion::{get_tty, raw::IntoRawMode, screen::IntoAlternateScreen};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
};
use tonic::transport::{Channel, Endpoint, Uri};
use tower::service_fn;

use sesh_proto::{sesh_client::SeshClient, sesh_kill_request::Session, SeshStartRequest, WinSize};

static mut EXIT: AtomicBool = AtomicBool::new(false);

#[derive(Debug, clap::Parser)]
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
    #[command(alias = "k")]
    /// Kill a session [alias: k]
    Kill {
        /// Id or name of session
        session: SessionSelector,
    },
    #[command(alias = "ls")]
    List,
    Shutdown,
}

#[derive(Debug, Clone)]
enum SessionSelector {
    Id(usize),
    Name(String),
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

async fn exec_session(
    client: SeshClient<Channel>,
    pid: i32,
    socket: String,
    name: String,
) -> Result<()> {
    let mut tty_output = get_tty()?.into_alternate_screen()?.into_raw_mode()?;
    tty_output.activate_raw_mode()?;

    let mut tty_input = tty_output.try_clone().unwrap();
    // let mut tty_input = stdin();

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
    let w_handle = tokio::task::spawn(async move {
        while unsafe { EXIT.load(Ordering::Relaxed) } == false {
            let mut packet = [0; 4096];

            let nbytes = tty_input.read(&mut packet)?;
            if nbytes == 0 {
                break;
            }
            let read = &packet[..nbytes];

            // Ctrl-\
            // TODO: Make this configurable

            if nbytes >= 2 && read[0] == 27 && read[1] == 92 {
                detach_session(client, None, Some(name)).await?;
                break;
            }

            w_stream.write_all(&read).await?;
            w_stream.flush().await?;
            // TODO: Use a less hacky method of reducing CPU usage
            // tokio::time::sleep(tokio::time::Duration::from_nanos(20)).await;
        }
        Result::<_, anyhow::Error>::Ok(())
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

    // the write handle will block if it's not aborted
    w_handle.abort();
    r_handle.await??;
    Ok(())
}

async fn start_session(
    mut client: SeshClient<Channel>,
    name: Option<String>,
    program: Option<String>,
    args: Vec<String>,
    attach: bool,
) -> anyhow::Result<()> {
    let program = program.unwrap_or_else(|| "bash".to_owned());
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
    Ok(())
}

async fn attach_session(mut client: SeshClient<Channel>, session: SessionSelector) -> Result<()> {
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
    Ok(())
}

async fn detach_session(
    mut client: SeshClient<Channel>,
    id: Option<usize>,
    name: Option<String>,
) -> Result<()> {
    use sesh_proto::sesh_detach_request::Session::*;
    let session = match (id, name) {
        (Some(id), None) => Id(id as u64),
        (None, Some(name)) => Name(name),
        _ => unreachable!("This should be unreachable due to CLI"),
    };
    let request = tonic::Request::new(sesh_proto::SeshDetachRequest {
        session: Some(session),
    });
    let response = client.detach_session(request).await?;
    unsafe {
        EXIT.store(true, Ordering::Relaxed);
    }
    if response.into_inner().success {
        println!("Session detached successfully.");
    } else {
        println!("Session not found.");
    }
    Ok(())
}

async fn kill_session(mut client: SeshClient<Channel>, session: SessionSelector) -> Result<()> {
    let request = tonic::Request::new(sesh_proto::SeshKillRequest {
        session: Some(match session {
            SessionSelector::Id(id) => Session::Id(id as u64),
            SessionSelector::Name(name) => Session::Name(name),
        }),
    });
    let response = client.kill_session(request).await?;
    if response.into_inner().success {
        println!("Session killed successfully.");
    } else {
        println!("Session not found.");
    }
    Ok(())
}

async fn list_sessions(mut client: SeshClient<Channel>) -> Result<()> {
    let request = tonic::Request::new(sesh_proto::SeshListRequest {});
    let response = client.list_sessions(request).await?.into_inner();
    let sessions = &response.sessions;

    for session in sessions.iter() {
        println!("◦ {}: {}", session.id, session.name);
    }
    Ok(())
}

async fn init_client(sock_path: PathBuf) -> Result<SeshClient<Channel>> {
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

    Ok(SeshClient::new(channel))
}

async fn shutdown_server(mut client: SeshClient<Channel>) -> Result<()> {
    let request = tonic::Request::new(sesh_proto::ShutdownServerRequest {});
    let response = client.shutdown_server(request).await?;
    if response.into_inner().success {
        println!("Server shutdown successfully.");
    } else {
        println!("Server shutdown failed.");
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ctrlc::set_handler(move || unsafe {
        EXIT.store(true, Ordering::Relaxed);
    })?;
    let args = Cli::parse();

    let rt = dirs::runtime_dir()
        .unwrap_or(PathBuf::from("/tmp/"))
        .join("sesh/");
    let server_sock = rt.join("server.sock");

    if !server_sock.exists() {
        let mut pty = Pty::spawn(
            &std::env::var("SESHD_PATH").unwrap_or("seshd".to_owned()),
            vec![],
            &Size::term_size()?,
        )?;
        pty.daemonize();
        while !server_sock.exists() {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }

    let client = init_client(server_sock).await?;

    match args.command.unwrap_or(Command::List) {
        Command::Start {
            name,
            program,
            args,
            detached,
        } => start_session(client, name, program, args, !detached).await?,
        Command::Kill { session } => kill_session(client, session).await?,
        Command::Attach { session } => attach_session(client, session).await?,
        Command::List => list_sessions(client).await?,
        Command::Shutdown => shutdown_server(client).await?,
    }

    // TODO: exit more cleanly
    std::process::exit(0);
}
