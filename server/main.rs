use anyhow::Result;
use log::info;

use session::Session;
use std::collections::HashMap;
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio::{
    net::UnixListener,
    signal::unix::{signal, SignalKind},
    sync::{mpsc::UnboundedSender, Mutex},
};
use tokio_stream::wrappers::UnixListenerStream;
use tonic::transport::Server as RPCServer;

use sesh_proto::{
    seshd_server::SeshdServer, SeshAttachRequest, SeshDetachRequest, SeshKillRequest,
    SeshResizeRequest, SeshStartRequest,
};

mod commands;
mod rpc;
mod session;
use commands::{Command, CommandResponse};

#[derive(Clone)]
struct Seshd {
    // TODO: Do I need to queue events?
    sessions: Arc<Mutex<HashMap<String, Session>>>,
    exit_signal: UnboundedSender<()>,
    runtime_dir: PathBuf,
}

impl Seshd {
    fn new(exit_signal: UnboundedSender<()>, runtime_dir: PathBuf) -> Result<Self> {
        let sessions = Arc::new(Mutex::new(HashMap::<String, Session>::new()));
        // Handle process exits
        // TODO: Send exit signal to connected clients
        tokio::task::spawn({
            let sessions = sessions.clone();
            let exit = exit_signal.clone();
            async move {
                let mut signal = signal(SignalKind::child())?;
                loop {
                    signal.recv().await;
                    let mut sessions = sessions.lock().await;
                    let mut to_remove = Vec::new();
                    for (name, session) in sessions.iter() {
                        let pid = session.pid();
                        let res = unsafe { libc::waitpid(pid, &mut 0, libc::WNOHANG) };
                        if res > 0 {
                            info!(
                                target: &format!("{}: {}", session.id, name),
                                "Subprocess {} exited", session.program
                            );
                            to_remove.push(name.clone());
                        }
                    }
                    // Remove sessions with exited processes
                    for name in to_remove {
                        sessions.remove(&name);
                    }
                    if sessions.is_empty() {
                        exit.send(())?;
                    }
                    // TODO: Use a less hacky method of reducing CPU usage
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                #[allow(unreachable_code)]
                Result::<_, anyhow::Error>::Ok(())
            }
        });
        info!(target: "rpc", "Server started");
        Ok(Self {
            sessions,
            exit_signal,
            runtime_dir,
        })
    }

    pub async fn exec(&self, cmd: Command) -> Result<CommandResponse> {
        match cmd {
            Command::ResizeSession(SeshResizeRequest { session, size }) => {
                self.exec_resize(session, size).await
            }
            Command::ListSessions => self.exec_list().await,
            Command::StartSession(SeshStartRequest {
                name,
                program,
                args,
                size,
            }) => self.exec_start(name, program, args, size).await,
            Command::AttachSession(SeshAttachRequest { session, size }) => {
                self.exec_attach(session, size).await
            }
            Command::DetachSession(SeshDetachRequest { session }) => {
                self.exec_detach(session).await
            }
            Command::KillSession(SeshKillRequest { session }) => self.exec_kill(session).await,
            Command::ShutdownServer => self.exec_shutdown().await,
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let runtime_dir = dirs::runtime_dir()
        .unwrap_or(PathBuf::from("/tmp/"))
        .join("sesh/");

    info!(target: "init", "Starting up");
    if !runtime_dir.exists() {
        info!(target: "init", "Creating runtime directory");
        std::fs::create_dir_all(&runtime_dir)?;
    }

    // Create the server socket
    info!(target: "init", "Creating server socket");
    let socket_path = runtime_dir.join("server.sock");
    let uds = UnixListener::bind(&socket_path)?;
    let uds_stream = UnixListenerStream::new(uds);

    let (exit_tx, mut exit_rx) = tokio::sync::mpsc::unbounded_channel::<()>();

    let sigint_tx = exit_tx.clone();
    let mut sigint = signal(SignalKind::interrupt())?;
    let sigquit_tx = exit_tx.clone();
    let mut sigquit = signal(SignalKind::quit())?;
    tokio::task::spawn(async move {
        tokio::select! {
            _ = sigint.recv() => {
                info!(target: "exit", "Received SIGINT");
                sigint_tx.send(()).ok();
            },
            _ = sigquit.recv() => {
                info!(target: "exit", "Received SIGQUIT");
                sigquit_tx.send(()).ok();
            }
        }
    });

    // Initialize the Tonic gRPC server
    info!(target: "init", "Setting up RPC server");
    RPCServer::builder()
        .add_service(SeshdServer::new(Seshd::new(exit_tx, runtime_dir)?))
        // .serve_with_incoming(uds_stream)
        .serve_with_incoming_shutdown(uds_stream, async move {
            exit_rx.recv().await;
        })
        .await?;

    info!(target: "exit", "Shutting down");
    // remove socket on exit
    std::fs::remove_file(&socket_path)?;

    Ok(())
}
