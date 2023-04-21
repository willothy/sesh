use std::{
    io::{stdin, Read, Write},
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

use anyhow::Result;
use clap::{Parser, Subcommand};
use termion::{get_tty, raw::IntoRawMode};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
};
use tonic::transport::{Channel, Endpoint, Uri};
use tower::service_fn;

use sesh_proto::{sesh_client::SeshClient, sesh_kill_request::Session, SeshStartRequest};

static mut EXIT: AtomicBool = AtomicBool::new(false);
const SOCK_PATH: &str = "/tmp/sesh/server.sock";

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
    },
    #[group(required = true, multiple = false)]
    #[command(alias = "a")]
    /// Attach to a session [alias: a]
    Attach {
        #[arg(short, long)]
        name: Option<String>,
        #[arg(short, long)]
        id: Option<usize>,
    },
    #[group(required = true, multiple = false)]
    #[command(alias = "k")]
    /// Kill a session [alias: k]
    Kill {
        #[arg(short, long)]
        name: Option<String>,
        #[arg(short, long)]
        id: Option<usize>,
    },
    #[command(alias = "ls")]
    List,
}

async fn start_session(
    mut client: SeshClient<Channel>,
    name: Option<String>,
    program: Option<String>,
    args: Vec<String>,
) -> anyhow::Result<()> {
    let program = program.unwrap_or_else(|| "bash".to_owned());
    let request = tonic::Request::new(SeshStartRequest {
        name: name.unwrap_or_else(|| program.clone()),
        program,
        args,
    });
    let response = client.start_session(request).await?.into_inner();
    let socket = response.socket;
    let pid = response.pid;
    let mut tty_output = get_tty().unwrap().into_raw_mode().unwrap();
    tty_output.activate_raw_mode()?;
    // let mut tty_input = tty_output.try_clone().unwrap();
    let mut tty_input = stdin();

    let (mut r_stream, mut w_stream) = UnixStream::connect(&socket).await?.into_split();

    let r_handle = tokio::task::spawn(async move {
        while unsafe { EXIT.load(Ordering::Relaxed) } == false {
            let mut packet = [0; 4096];

            let nbytes = r_stream.read(&mut packet).await?;
            let read = &packet[..nbytes];
            tty_output.write_all(&read)?;
            tty_output.flush()?;
            // TODO: Use a less hacky method of reducing CPU usage
            tokio::time::sleep(tokio::time::Duration::from_nanos(200)).await;
        }
        Result::<_, anyhow::Error>::Ok(())
    });
    tokio::task::spawn(async move {
        while unsafe { EXIT.load(Ordering::Relaxed) } == false {
            let mut packet = [0; 4096];

            let nbytes = tty_input.read(&mut packet)?;
            let read = &packet[..nbytes];
            w_stream.write_all(&read).await?;
            w_stream.flush().await?;
            // TODO: Use a less hacky method of reducing CPU usage
            // tokio::time::sleep(tokio::time::Duration::from_nanos(200)).await;
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

    // w_handle.abort();
    r_handle.await??;
    Ok(())
}

async fn kill_session(
    mut client: SeshClient<Channel>,
    id: Option<usize>,
    name: Option<String>,
) -> Result<()> {
    let session = match (id, name) {
        (Some(id), None) => Session::Id(id as u64),
        (None, Some(name)) => Session::Name(name),
        _ => unreachable!("This should be unreachable due to CLI"),
    };
    let request = tonic::Request::new(sesh_proto::SeshKillRequest {
        session: Some(session),
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

async fn init_client() -> Result<SeshClient<Channel>> {
    if !Path::new(SOCK_PATH).exists() {
        return Err(anyhow::anyhow!("Server socket not found at {}", SOCK_PATH));
    }

    // Create a channel to the server socket
    let channel = Endpoint::try_from("http://[::]:50051")?
        .connect_with_connector(service_fn(|_: Uri| {
            // Connect to a Uds socket
            UnixStream::connect(SOCK_PATH)
        }))
        .await?;

    Ok(SeshClient::new(channel))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ctrlc::set_handler(move || unsafe {
        EXIT.store(true, Ordering::Relaxed);
    })?;
    let args = Cli::parse();

    let client = init_client().await?;

    match args.command.unwrap_or(Command::List) {
        Command::Start {
            name,
            program,
            args,
        } => start_session(client, name, program, args).await?,
        Command::Kill {
            name: None,
            id: Some(id),
        } => kill_session(client, Some(id), None).await?,
        Command::Kill {
            name: Some(name),
            id: None,
        } => kill_session(client, None, Some(name)).await?,
        Command::List => list_sessions(client).await?,
        _ => {
            println!("Invalid command");
            return Ok(());
        }
    }

    // TODO: exit more cleanly
    std::process::exit(0);
}
