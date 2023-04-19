use clap::{Parser, Subcommand};
use tokio::net::UnixStream;
use tonic::transport::{Endpoint, Uri};
use tower::service_fn;

use sesh_proto::{sesh_client::SeshClient, SeshStartRequest};

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
            let response = client.start_session(request).await?;

            println!("RESPONSE={:?}", response.into_inner());
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
