use anyhow::Result;
use dashmap::DashMap;
use log::info;

use session::Session;
use std::{path::PathBuf, sync::Arc};
use tokio::{
    net::UnixListener,
    signal::unix::{signal, SignalKind},
    sync::mpsc::Sender,
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

pub const EXIT_ON_EMPTY: bool = true;

struct SessionList {
    sessions: DashMap<String, Session>,
    lookup: DashMap<usize, String>,
}

impl SessionList {
    pub fn new() -> Self {
        Self {
            sessions: DashMap::new(),
            lookup: DashMap::new(),
        }
    }

    /// Returns the number of sessions
    pub fn count(&self) -> usize {
        self.sessions.len()
    }

    /// Checks if a session exists by name
    pub fn contains(&self, name: impl AsRef<str>) -> bool {
        self.sessions.contains_key(name.as_ref())
    }

    /// Gets a session by name
    pub fn get(&self, name: impl AsRef<str>) -> Option<dashmap::mapref::one::Ref<String, Session>> {
        self.sessions.get(name.as_ref())
    }

    /// Gets a session by id
    pub fn get_by_id(&self, id: usize) -> Option<dashmap::mapref::one::Ref<String, Session>> {
        self.lookup
            .get(&id)
            .and_then(|name| self.sessions.get(name.as_str()))
    }

    /// Inserts a session into the list
    pub fn insert(&self, name: String, session: Session) {
        self.lookup.insert(session.id, name.clone());
        self.sessions.insert(name, session);
    }

    /// Removes a session by name
    pub fn remove(&self, name: impl AsRef<str>) -> Option<Session> {
        self.sessions.remove(name.as_ref()).map(|(_, session)| {
            self.lookup.remove(&session.id);
            session
        })
    }

    /// Removes sessions with exited processes
    pub fn clean(&self) -> bool {
        self.sessions.retain(|name, session| {
            let pid = session.pid();
            let res = unsafe { libc::waitpid(pid, &mut 0, libc::WNOHANG) };
            if res > 0 {
                info!(
                    target: &format!("{}: {}", session.id, name),
                    "Subprocess {} exited", session.program
                );
            }
            return res <= 0;
        });
        self.lookup
            .retain(|_, name| self.sessions.contains_key(name));
        return self.sessions.is_empty();
    }

    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }

    pub fn iter(
        &self,
    ) -> impl Iterator<Item = dashmap::mapref::multiple::RefMulti<String, Session>> {
        self.sessions.iter()
    }
}

struct Seshd {
    sessions: Arc<SessionList>,
    exit_signal: Sender<()>,
    runtime_dir: PathBuf,
}

impl Seshd {
    fn new(exit_signal: Sender<()>, runtime_dir: PathBuf) -> Result<Self> {
        let sessions = Arc::new(SessionList::new());
        // Handle process exits
        tokio::task::spawn({
            let sessions = Arc::clone(&sessions);
            let exit = exit_signal.clone();
            async move {
                let mut signal = signal(SignalKind::child())?;
                loop {
                    signal.recv().await;
                    if sessions.clean() && EXIT_ON_EMPTY {
                        exit.send(()).await?;
                        break;
                    }
                }
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
                pwd,
                env,
            }) => {
                self.exec_start(
                    name,
                    program,
                    args,
                    size,
                    pwd,
                    env.into_iter().map(|v| (v.key, v.value)).collect(),
                )
                .await
            }
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

    let (exit_tx, mut exit_rx) = tokio::sync::mpsc::channel::<()>(1);

    let sigint_tx = exit_tx.clone();
    let mut sigint = signal(SignalKind::interrupt())?;
    let sigquit_tx = exit_tx.clone();
    let mut sigquit = signal(SignalKind::quit())?;
    tokio::task::spawn(async move {
        tokio::select! {
            _ = sigint.recv() => {
                info!(target: "exit", "Received SIGINT");
                sigint_tx.send(()).await.ok();
            },
            _ = sigquit.recv() => {
                info!(target: "exit", "Received SIGQUIT");
                sigquit_tx.send(()).await.ok();
            }
        }
    });

    // Initialize the Tonic gRPC server
    info!(target: "init", "Setting up RPC server");
    RPCServer::builder()
        .add_service(SeshdServer::new(Seshd::new(exit_tx, runtime_dir)?))
        .serve_with_incoming_shutdown(uds_stream, async move {
            exit_rx.recv().await;
        })
        .await?;

    info!(target: "exit", "Shutting down");
    // remove socket on exit
    std::fs::remove_file(&socket_path)?;

    Ok(())
}
