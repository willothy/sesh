use anyhow::{Context, Result};
use log::{info, warn};
use sesh_shared::{error::CResult, pty::Pty, term::Size};
use std::{
    collections::HashMap,
    os::fd::{AsRawFd, FromRawFd, RawFd},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{
    fs,
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixListener,
    signal::unix::{signal, SignalKind},
    sync::{mpsc::UnboundedSender, Mutex},
};
use tokio_stream::wrappers::UnixListenerStream;
use tonic::{transport::Server as RPCServer, Request, Response, Status};

use sesh_proto::{
    sesh_server::{Sesh as RPCDefs, SeshServer},
    SeshAttachRequest, SeshAttachResponse, SeshDetachRequest, SeshDetachResponse, SeshKillRequest,
    SeshKillResponse, SeshListResponse, SeshStartRequest, SeshStartResponse, ShutdownServerRequest,
    ShutdownServerResponse,
};

mod commands;
use commands::{Command, CommandResponse};

pub const SERVER_SOCK: &str = "/tmp/sesh/server.sock";

struct Session {
    id: usize,
    name: String,
    program: String,
    pty: Pty,
    // socket: Arc<Mutex<UnixStream>>,
    sock_path: PathBuf,
    connected: Arc<AtomicBool>,
}

impl Session {
    fn new(id: usize, name: String, program: String, pty: Pty, sock_path: PathBuf) -> Self {
        Self {
            id,
            name,
            program,
            pty,
            sock_path,
            connected: Arc::new(AtomicBool::new(false)),
        }
    }

    fn pid(&self) -> i32 {
        self.pty.pid()
    }

    async fn start(
        sock_path: PathBuf,
        fd: RawFd,
        connected: Arc<AtomicBool>,
        size: Size,
    ) -> Result<()> {
        let socket = UnixListener::bind(&sock_path)?;
        info!("Listening on {:?}", sock_path);
        let (stream, _addr) = socket.accept().await?;
        info!("Accepted connection from {:?}", _addr);
        connected.store(true, Ordering::Release);

        let (mut r_socket, mut w_socket) = stream.into_split();

        let pty = unsafe { tokio::fs::File::from_raw_fd(fd) };
        unsafe {
            libc::ioctl(
                fd,
                libc::TIOCSWINSZ,
                &Into::<libc::winsize>::into(&sesh_shared::term::Size {
                    rows: size.rows,
                    cols: size.cols,
                }),
            )
            .to_result()
            .map(|_| ())
            .context("Failed to resize")?;
        }

        let w_handle = tokio::task::spawn({
            let connected = connected.clone();
            let mut pty = pty.try_clone().await?;
            async move {
                info!("Starting pty write loop");
                while connected.load(Ordering::Relaxed) == true {
                    let mut i_packet = [0; 4096];

                    let i_count = pty.read(&mut i_packet).await?;
                    if i_count == 0 {
                        warn!("Read 0, exiting pty write loop");
                        connected.store(false, Ordering::Relaxed);
                        w_socket.flush().await?;
                        break;
                    }
                    info!("Read {} bytes from pty", i_count);
                    let read = &i_packet[..i_count];
                    w_socket.write_all(&read).await?;
                    w_socket.flush().await?;
                    // TODO: Use a less hacky method of reducing CPU usage
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
                info!("Exiting pty read loop");
                Result::<_, anyhow::Error>::Ok(())
            }
        });
        tokio::task::spawn({
            let connected = connected.clone();
            let mut pty = pty.try_clone().await?;
            async move {
                info!("Starting socket read loop");
                while connected.load(Ordering::Relaxed) == true {
                    let mut o_packet = [0; 4096];

                    let o_count = r_socket.read(&mut o_packet).await?;
                    if o_count == 0 {
                        warn!("Read 0, exiting socket read loop");
                        connected.store(false, Ordering::Relaxed);
                        w_handle.abort();
                        pty.flush().await?;
                        break;
                    }
                    info!("Read {} bytes from socket", o_count);
                    let read = &o_packet[..o_count];
                    pty.write_all(&read).await?;
                    pty.flush().await?;
                    // TODO: Use a less hacky method of reducing CPU usage
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
                info!("Exiting socket read loop");

                Result::<_, anyhow::Error>::Ok(())
            }
        });
        info!("Started session on {}", sock_path.display());
        Ok(())
    }

    async fn detach(&self) -> Result<()> {
        info!("Detaching session {}", self.id);
        self.connected.store(false, Ordering::Relaxed);
        fs::remove_file(&self.sock_path).await?;
        Ok(())
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        // get rid of the socket
        std::fs::remove_file(&self.sock_path).ok();
    }
}

#[derive(Clone)]
struct Sesh {
    // do I need to queue events?
    // just uncomment the queue related things, it's working already though probably not optimal
    // queue: Arc<Mutex<VecDeque<Command>>>,
    sessions: Arc<Mutex<HashMap<String, Session>>>,
    exit_signal: UnboundedSender<()>,
}

#[tonic::async_trait]
impl RPCDefs for Sesh {
    async fn start_session(
        &self,
        request: Request<SeshStartRequest>,
    ) -> Result<Response<SeshStartResponse>, Status> {
        let req = request.into_inner();

        let res = self.exec(Command::StartSession(req)).await;

        match res {
            Ok(CommandResponse::StartSession(response)) => Ok(Response::new(response)),
            Ok(_) => Err(Status::internal("Unexpected response")),
            Err(e) => Err(Status::internal(format!("Error: {}", e))),
        }
    }

    async fn attach_session(
        &self,
        request: Request<sesh_proto::SeshAttachRequest>,
    ) -> Result<Response<sesh_proto::SeshAttachResponse>, Status> {
        let req = request.into_inner();

        let res = self.exec(Command::AttachSession(req)).await;

        match res {
            Ok(CommandResponse::AttachSession(response)) => Ok(Response::new(response)),
            Ok(_) => Err(Status::internal("Unexpected response")),
            Err(e) => Err(Status::internal(format!("Error: {}", e))),
        }
    }

    async fn detach_session(
        &self,
        request: Request<sesh_proto::SeshDetachRequest>,
    ) -> Result<Response<sesh_proto::SeshDetachResponse>, Status> {
        let req = request.into_inner();

        let res = self.exec(Command::DetachSession(req)).await;

        match res {
            Ok(CommandResponse::DetachSession(response)) => Ok(Response::new(response)),
            Ok(_) => Err(Status::internal("Unexpected response")),
            Err(e) => Err(Status::internal(format!("Error: {}", e))),
        }
    }

    async fn kill_session(
        &self,
        request: Request<SeshKillRequest>,
    ) -> Result<Response<SeshKillResponse>, Status> {
        let req = request.into_inner();

        let res = self.exec(Command::KillSession(req)).await;

        match res {
            Ok(CommandResponse::KillSession(response)) => Ok(Response::new(response)),
            Ok(_) => Err(Status::internal("Unexpected response")),
            Err(e) => Err(Status::internal(format!("Error: {}", e))),
        }
    }

    async fn list_sessions(
        &self,
        _: Request<sesh_proto::SeshListRequest>,
    ) -> Result<Response<sesh_proto::SeshListResponse>, Status> {
        let res = self.exec(Command::ListSessions).await;

        match res {
            Ok(CommandResponse::ListSessions(response)) => Ok(Response::new(response)),
            Ok(_) => Err(Status::internal("Unexpected response")),
            Err(e) => Err(Status::internal(format!("Error: {}", e))),
        }
    }

    async fn shutdown_server(
        &self,
        _: tonic::Request<ShutdownServerRequest>,
    ) -> Result<Response<ShutdownServerResponse>, Status> {
        let res = self.exec(Command::ShutdownServer).await;

        match res {
            Ok(CommandResponse::ShutdownServer(response)) => Ok(Response::new(response)),
            Ok(_) => Err(Status::internal("Unexpected response")),
            Err(e) => Err(Status::internal(format!("Error: {}", e))),
        }
    }
}

impl Sesh {
    fn new(exit_signal: UnboundedSender<()>) -> Result<Self> {
        let sessions = Arc::new(Mutex::new(HashMap::<String, Session>::new()));
        // Handle process exits
        // TODO: Send exit signal to connected clients
        tokio::task::spawn({
            let sessions = sessions.clone();
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
                            info!("{}: {} - Subprocess exited.", session.id, name);
                            to_remove.push(name.clone());
                        }
                    }
                    // Remove sessions with exited processes
                    for name in to_remove {
                        sessions.remove(&name);
                    }
                    // TODO: Use a less hacky method of reducing CPU usage
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                #[allow(unreachable_code)]
                Result::<_, anyhow::Error>::Ok(())
            }
        });
        Ok(Self {
            sessions,
            exit_signal,
        })
    }

    pub async fn exec(&self, cmd: Command) -> Result<CommandResponse> {
        match cmd {
            Command::ListSessions => {
                let sessions = self.sessions.lock().await;
                let sessions = sessions
                    .iter()
                    .map(|(name, session)| sesh_proto::SeshInfo {
                        id: session.id as u64,
                        name: name.clone(),
                        program: session.program.clone(),
                    })
                    .collect();
                Ok(CommandResponse::ListSessions(SeshListResponse { sessions }))
            }
            Command::StartSession(SeshStartRequest {
                name,
                program,
                args,
                size,
            }) => {
                let pty = Pty::spawn(&program, args, &Size::term_size()?)
                    .map_err(|e| anyhow::anyhow!("{:?}", e))?;
                let size = if let Some(size) = size {
                    Size {
                        rows: size.rows as u16,
                        cols: size.cols as u16,
                    }
                } else {
                    Size::term_size()?
                };
                pty.resize(&size)?;

                let mut sessions = self.sessions.lock().await;
                let session_id = sessions.len();
                let pid = pty.pid();

                let name = PathBuf::from(&name)
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or(name.replace("/", "_"));

                let mut session_name = name.clone();
                let mut i = 0;
                while sessions.contains_key(&session_name) {
                    session_name = format!("{}-{}", name, i);
                    i += 1;
                }

                let socket_path = format!("/tmp/sesh/{}.sock", &session_name);

                let session = Session::new(
                    session_id,
                    session_name.clone(),
                    program,
                    pty,
                    PathBuf::from(&socket_path),
                );
                info!(
                    "{}:{} - Starting on {}",
                    session.id,
                    &session.name,
                    &session.sock_path.display()
                );
                tokio::task::spawn({
                    let socket = session.sock_path.clone();
                    // let file = session.pty.file().try_clone().await?;
                    let file = session.pty.file().as_raw_fd();
                    // Duplicate FD
                    // I do not know why this makes the socket connection not die, but it does
                    let file = unsafe { libc::fcntl(file, libc::F_DUPFD, file) };
                    let connected = session.connected.clone();
                    async move {
                        Session::start(socket, file, connected, size).await?;
                        Result::<_, anyhow::Error>::Ok(())
                    }
                });

                sessions.insert(session.name.clone(), session);
                Ok(CommandResponse::StartSession(SeshStartResponse {
                    socket: socket_path,
                    name: session_name,
                    pid,
                }))
            }
            Command::AttachSession(SeshAttachRequest { session, size }) => {
                if let Some(session) = session {
                    let sessions = self.sessions.lock().await;
                    let session = match session {
                        sesh_proto::sesh_attach_request::Session::Name(name) => sessions.get(&name),
                        sesh_proto::sesh_attach_request::Session::Id(id) => sessions
                            .iter()
                            .find(|(_, s)| s.id == id as usize)
                            .map(|(_, s)| s),
                    }
                    .ok_or_else(|| anyhow::anyhow!("Session not found"))?;
                    let size = if let Some(size) = size {
                        Size {
                            rows: size.rows as u16,
                            cols: size.cols as u16,
                            ..Size::term_size()?
                        }
                    } else {
                        Size::term_size()?
                    };
                    tokio::task::spawn({
                        let socket = session.sock_path.clone();
                        let file = session.pty.file().as_raw_fd();
                        let file = unsafe { libc::fcntl(file, libc::F_DUPFD, file) };
                        let connected = session.connected.clone();
                        async move {
                            Session::start(socket, file, connected, size).await?;
                            Result::<_, anyhow::Error>::Ok(())
                        }
                    });

                    Ok(CommandResponse::AttachSession(SeshAttachResponse {
                        socket: session.sock_path.to_string_lossy().to_string(),
                        pid: session.pid(),
                        name: session.name.clone(),
                    }))
                } else {
                    anyhow::bail!("No session specified");
                }
            }
            Command::DetachSession(SeshDetachRequest { session }) => {
                if let Some(session) = session {
                    let sessions = self.sessions.lock().await;
                    let name = match session {
                        sesh_proto::sesh_detach_request::Session::Name(name) => Some(name),
                        sesh_proto::sesh_detach_request::Session::Id(id) => {
                            let name = sessions
                                .iter()
                                .find(|(_, s)| s.id == id as usize)
                                .map(|(_, s)| s.name.clone());
                            name
                        }
                    };

                    if let Some(name) = name {
                        if let Some(session) = sessions.get(&name) {
                            session.detach().await?;
                        }
                    }
                }
                Ok(CommandResponse::DetachSession(SeshDetachResponse {
                    success: true,
                }))
            }
            Command::KillSession(request) => {
                if let Some(session) = request.session {
                    let mut sessions = self.sessions.lock().await;
                    let name = match session {
                        sesh_proto::sesh_kill_request::Session::Name(name) => Some(name),
                        sesh_proto::sesh_kill_request::Session::Id(id) => {
                            let name = sessions
                                .iter()
                                .find(|(_, s)| s.id == id as usize)
                                .map(|(_, s)| s.name.clone());
                            name
                        }
                    };

                    let success = if let Some(name) = name {
                        if let Some(session) = sessions.remove(&name) {
                            info!("{}:{} - Killing.", session.id, &session.name);
                            true
                        } else {
                            false
                        }
                    } else {
                        false
                    };
                    Ok(CommandResponse::KillSession(SeshKillResponse { success }))
                } else {
                    // TODO: Kill the *current* session and exit?
                    Ok(CommandResponse::KillSession(SeshKillResponse {
                        success: false,
                    }))
                }
            }
            Command::ShutdownServer => {
                info!("Shutting down server");
                self.exit_signal.send(())?;
                return Ok(CommandResponse::ShutdownServer(ShutdownServerResponse {
                    success: true,
                }));
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    info!("Starting up");
    let socket_path = PathBuf::from(SERVER_SOCK);
    let parent_dir = socket_path.parent();
    if let Some(parent) = parent_dir {
        // Ensure /tmp/sesh/ exists
        std::fs::create_dir_all(parent)?;
    }

    // Create the server socket
    let uds = UnixListener::bind(SERVER_SOCK)?;
    let uds_stream = UnixListenerStream::new(uds);

    let (exit_tx, mut exit_rx) = tokio::sync::mpsc::unbounded_channel::<()>();

    let sigint_tx = exit_tx.clone();
    ctrlc::set_handler(move || {
        sigint_tx.send(()).ok();
    })?;

    // Initialize the Tonic gRPC server
    RPCServer::builder()
        .add_service(SeshServer::new(Sesh::new(exit_tx)?))
        // .serve_with_incoming(uds_stream)
        .serve_with_incoming_shutdown(uds_stream, async move {
            // Shutdown on sigint
            // TODO: Create shutdown command for server

            // tokio::signal::ctrl_c().await.ok();
            exit_rx.recv().await;
        })
        .await?;

    // remove socket on exit
    std::fs::remove_file(&socket_path)?;

    Ok(())
}
