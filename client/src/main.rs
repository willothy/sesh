use std::{
    io::{Read, Write},
    sync::atomic::{AtomicBool, Ordering},
};

use clap::{Parser, Subcommand};
use termion::{get_tty, raw::IntoRawMode};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
};
use tonic::transport::{Endpoint, Uri};
use tower::service_fn;

use sesh_proto::{sesh_client::SeshClient, SeshStartRequest};

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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ctrlc::set_handler(move || unsafe {
        EXIT.store(true, Ordering::Relaxed);
    })?;

    let args = Cli::parse();

    // Create a channel to the server socket
    let channel = Endpoint::try_from("http://[::]:50051")?
        .connect_with_connector(service_fn(|_: Uri| {
            let path = "/tmp/sesh/server.sock";

            // Connect to a Uds socket
            UnixStream::connect(path)
        }))
        .await?;

    let mut client = SeshClient::new(channel);

    match args.command.unwrap_or(Command::List) {
        Command::Start {
            name,
            program,
            args,
        } => {
            let program = program.unwrap_or_else(|| "bash".to_owned());
            let request = tonic::Request::new(SeshStartRequest {
                name: name.unwrap_or_else(|| program.clone()),
                program,
                args,
            });
            let socket = client.start_session(request).await?.into_inner().socket;
            let mut tty_output = get_tty().unwrap().into_raw_mode().unwrap();
            let mut tty_input = tty_output.try_clone().unwrap();

            let (mut r_stream, mut w_stream) = UnixStream::connect(&socket).await?.into_split();

            let handle = tokio::task::spawn(async move {
                while unsafe { EXIT.swap(false, Ordering::Relaxed) } == false {
                    let mut i_packet = [0; 4096];

                    let i_count = r_stream.read(&mut i_packet).await?;
                    let read = &i_packet[..i_count];
                    tty_output.write_all(&read)?;
                    tty_output.flush()?;
                    tokio::task::yield_now().await;
                }
                #[allow(unreachable_code)]
                Result::<_, anyhow::Error>::Ok(())
            });

            while unsafe { EXIT.swap(false, Ordering::Relaxed) } == false {
                let mut o_packet = [0; 4096];

                let o_count = tty_input.read(&mut o_packet)?;
                let read = &o_packet[..o_count];
                w_stream.write_all(&read).await?;
                w_stream.flush().await?;
            }
            handle.await??;
        }
        Command::Kill {
            name: None,
            id: Some(id),
        } => {
            let request = tonic::Request::new(sesh_proto::SeshKillRequest {
                session: Some(sesh_proto::sesh_kill_request::Session::Id(id as u64)),
            });
            let response = client.kill_session(request).await?;

            println!("RESPONSE={:?}", response.into_inner());
        }
        Command::Kill {
            name: Some(name),
            id: None,
        } => {
            let request = tonic::Request::new(sesh_proto::SeshKillRequest {
                session: Some(sesh_proto::sesh_kill_request::Session::Name(name)),
            });
            let response = client.kill_session(request).await?;

            println!("RESPONSE={:?}", response.into_inner());
        }
        Command::List => {
            let request = tonic::Request::new(sesh_proto::SeshListRequest {});
            let response = client.list_sessions(request).await?;

            println!("RESPONSE={:?}", response.into_inner());
        }
        _ => {
            println!("Invalid command");
            return Ok(());
        }
    }

    Ok(())
}
