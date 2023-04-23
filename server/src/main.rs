use anyhow::{Context, Result};
use log::{error, info, trace};
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
    io::{AsyncReadExt, AsyncWriteExt},
    net::{UnixListener, UnixStream},
    signal::unix::{signal, SignalKind},
    sync::{mpsc::UnboundedSender, Mutex},
};
use tokio_stream::wrappers::UnixListenerStream;
use tonic::{
    transport::{Endpoint, Server as RPCServer, Uri},
    Request, Response, Status,
};
use tower::service_fn;

use sesh_proto::{
    sesh_cli_client::SeshCliClient,
    seshd_server::{Seshd as RPCDefs, SeshdServer},
    ClientDetachRequest, SeshAttachRequest, SeshAttachResponse, SeshDetachRequest,
    SeshDetachResponse, SeshKillRequest, SeshKillResponse, SeshListResponse, SeshResizeRequest,
    SeshResizeResponse, SeshStartRequest, SeshStartResponse, ShutdownServerRequest,
    ShutdownServerResponse,
};

mod commands;
use commands::{Command, CommandResponse};

struct Session {
    id: usize,
    name: String,
    program: String,
    pty: Pty,
    connected: Arc<AtomicBool>,
    listener: Arc<UnixListener>,
    sock_path: PathBuf,
}

impl Session {
    fn new(id: usize, name: String, program: String, pty: Pty, sock_path: PathBuf) -> Result<Self> {
        Ok(Self {
            id,
            name,
            program,
            pty,
            connected: Arc::new(AtomicBool::new(false)),
            listener: Arc::new(UnixListener::bind(&sock_path)?),
            sock_path,
        })
    }

    fn log_group(&self) -> String {
        format!("{}: {}", self.id, self.name)
    }

    fn pid(&self) -> i32 {
        self.pty.pid()
    }

    async fn start(
        sock_path: PathBuf,
        socket: Arc<UnixListener>,
        fd: RawFd,
        connected: Arc<AtomicBool>,
        size: Size,
    ) -> Result<()> {
        info!(target: "session", "Listening on {:?}", sock_path);
        let (stream, _addr) = socket.accept().await?;
        info!(target: "session", "Accepted connection from {:?}", _addr);
        connected.store(true, Ordering::Release);

        let (mut r_socket, mut w_socket) = stream.into_split();

        let pty = unsafe { tokio::fs::File::from_raw_fd(fd) };
        unsafe {
            libc::ioctl(
                fd,
                libc::TIOCSWINSZ,
                &Into::<libc::winsize>::into(&sesh_shared::term::Size {
                    rows: size.rows,
                    cols: size.cols - 1,
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
                info!(target: "session", "Starting pty write loop");
                while connected.load(Ordering::Relaxed) == true {
                    let mut i_packet = [0; 4096];

                    let i_count = pty.read(&mut i_packet).await?;
                    if i_count == 0 {
                        connected.store(false, Ordering::Relaxed);
                        w_socket.flush().await?;
                        pty.flush().await?;
                        break;
                    }
                    trace!(target: "session", "Read {} bytes from pty", i_count);
                    let read = &i_packet[..i_count];
                    w_socket.write_all(&read).await?;
                    w_socket.flush().await?;
                    // TODO: Use a less hacky method of reducing CPU usage
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
                info!(target: "session","Exiting pty read loop");
                Result::<_, anyhow::Error>::Ok(())
            }
        });
        tokio::task::spawn({
            let connected = connected.clone();
            let mut pty = pty.try_clone().await?;
            async move {
                info!(target: "session","Starting socket read loop");
                while connected.load(Ordering::Relaxed) == true {
                    let mut o_packet = [0; 4096];

                    let o_count = r_socket.read(&mut o_packet).await?;
                    if o_count == 0 {
                        connected.store(false, Ordering::Relaxed);
                        w_handle.abort();
                        // pty.flush().await?;
                        break;
                    }
                    trace!(target: "session", "Read {} bytes from socket", o_count);
                    let read = &o_packet[..o_count];
                    pty.write_all(&read).await?;
                    pty.flush().await?;
                    // TODO: Use a less hacky method of reducing CPU usage
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
                info!(target: "session","Exiting socket and pty read loops");

                Result::<_, anyhow::Error>::Ok(())
            }
        });
        info!(target: "session", "Started {}", sock_path.display());
        Ok(())
    }

    async fn detach(&self) -> Result<()> {
        self.connected.store(false, Ordering::Relaxed);
        let parent = self
            .sock_path
            .parent()
            .ok_or(anyhow::anyhow!("No parent"))?;
        let client_sock_path = parent.join(format!("client-{}.sock", self.pid()));

        let channel = Endpoint::try_from("http://[::]:50051")?
            .connect_with_connector(service_fn(move |_: Uri| {
                UnixStream::connect(client_sock_path.clone())
            }))
            .await?;
        let mut client = SeshCliClient::new(channel);

        client.detach(ClientDetachRequest {}).await?;

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
struct Seshd {
    // TODO: Do I need to queue events?
    sessions: Arc<Mutex<HashMap<String, Session>>>,
    exit_signal: UnboundedSender<()>,
    runtime_dir: PathBuf,
}

#[tonic::async_trait]
impl RPCDefs for Seshd {
    async fn start_session(
        &self,
        request: Request<SeshStartRequest>,
    ) -> Result<Response<SeshStartResponse>, Status> {
        let req = request.into_inner();

        let res = self.exec(Command::StartSession(req)).await;

        match res {
            Ok(CommandResponse::StartSession(response)) => Ok(Response::new(response)),
            Ok(_) => Err(Status::internal("Unexpected response")),
            Err(e) => {
                let err_s = format!("{}", e);
                error!(target: "rpc", "{}", err_s);
                Err(Status::internal(err_s))
            }
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
            Err(e) => {
                let err_s = format!("{}", e);
                error!(target: "rpc", "{}", err_s);
                Err(Status::internal(err_s))
            }
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
            Err(e) => {
                let err_s = format!("{}", e);
                error!(target: "rpc", "{}", err_s);
                Err(Status::internal(err_s))
            }
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
            Err(e) => {
                let err_s = format!("{}", e);
                error!(target: "rpc", "{}", err_s);
                Err(Status::internal(err_s))
            }
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
            Err(e) => {
                let err_s = format!("{}", e);
                error!(target: "rpc", "{}", err_s);
                Err(Status::internal(err_s))
            }
        }
    }

    async fn resize_session(
        &self,
        request: Request<SeshResizeRequest>,
    ) -> Result<Response<SeshResizeResponse>, Status> {
        let req = request.into_inner();

        let res = self.exec(Command::ResizeSession(req)).await;

        match res {
            Ok(CommandResponse::ResizeSession(res)) => Ok(Response::new(res)),
            Ok(_) => Err(Status::internal("Unexpected response")),
            Err(e) => {
                let err_s = format!("{}", e);
                error!(target: "rpc", "{}", err_s);
                Err(Status::internal(err_s))
            }
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
            Err(e) => {
                let err_s = format!("{}", e);
                error!(target: "rpc", "{}", err_s);
                Err(Status::internal(err_s))
            }
        }
    }
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
                let Some(size) = size else {
                    return Err(anyhow::anyhow!("Invalid size"));
                };
                let sessions = self.sessions.lock().await;
                let Some(session) = session else {
                    return Err(anyhow::anyhow!("Session not found"));
                };
                let Some(name) = (match session {
                    sesh_proto::sesh_resize_request::Session::Name(name) => Some(name),
                    sesh_proto::sesh_resize_request::Session::Id(id) => {
                        let name = sessions
                            .iter()
                            .find(|(_, s)| s.id == id as usize)
                            .map(|(_, s)| s.name.clone());
                        name
                    }
                }) else {
                    return Err(anyhow::anyhow!("Session not found"));
                };
                let session = sessions
                    .get(&name)
                    .ok_or_else(|| anyhow::anyhow!("Session not found: {}", name))?;
                info!(target: &session.log_group(), "Resizing");

                session.pty.resize(&Size {
                    cols: size.cols as u16,
                    rows: size.rows as u16,
                })?;
                Ok(CommandResponse::ResizeSession(SeshResizeResponse {}))
            }
            Command::ListSessions => {
                info!(target: "exec", "Listing sessions");
                let sessions = self.sessions.lock().await;
                let sessions = sessions
                    .iter()
                    .map(|(name, session)| sesh_proto::SeshInfo {
                        id: session.id as u64,
                        name: name.clone(),
                        program: session.program.clone(),
                        connected: session.connected.load(Ordering::Relaxed),
                    })
                    .collect::<Vec<_>>();
                if sessions.is_empty() {
                    self.exit_signal.clone().send(())?;
                }
                Ok(CommandResponse::ListSessions(SeshListResponse { sessions }))
            }
            Command::StartSession(SeshStartRequest {
                name,
                program,
                args,
                size,
            }) => {
                let mut sessions = self.sessions.lock().await;
                let session_id = sessions.len();

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

                let socket_path = self.runtime_dir.join(format!("{}.sock", session_name));

                let pty = Pty::new(&program)
                    .args(args)
                    .env("SESH_SESSION", socket_path.clone())
                    .env("SESH_NAME", session_name.clone())
                    .spawn(&Size::term_size()?)?;

                let pid = pty.pid();
                let size = if let Some(size) = size {
                    Size {
                        rows: size.rows as u16,
                        cols: size.cols as u16,
                    }
                } else {
                    Size::term_size()?
                };
                pty.resize(&size)?;

                let session = Session::new(
                    session_id,
                    session_name.clone(),
                    program,
                    pty,
                    PathBuf::from(&socket_path),
                )?;
                info!(target: &session.log_group(), "Starting on {}", session.sock_path.display());
                tokio::task::spawn({
                    let sock_path = session.sock_path.clone();
                    let socket = session.listener.clone();
                    // let file = session.pty.file().try_clone().await?;
                    let file = session.pty.file().as_raw_fd();
                    // Duplicate FD
                    // I do not know why this makes the socket connection not die, but it does
                    let file = unsafe { libc::fcntl(file, libc::F_DUPFD, file) };
                    let connected = session.connected.clone();
                    async move {
                        Session::start(sock_path, socket, file, connected, size).await?;
                        Result::<_, anyhow::Error>::Ok(())
                    }
                });

                sessions.insert(session.name.clone(), session);
                Ok(CommandResponse::StartSession(SeshStartResponse {
                    socket: socket_path.to_string_lossy().to_string(),
                    name: session_name,
                    pid,
                }))
            }
            Command::AttachSession(SeshAttachRequest { session, size }) => {
                if let Some(session) = session {
                    let sessions = self.sessions.lock().await;
                    let session = match &session {
                        sesh_proto::sesh_attach_request::Session::Name(name) => sessions.get(name),
                        sesh_proto::sesh_attach_request::Session::Id(id) => sessions
                            .iter()
                            .find(|(_, s)| s.id == *id as usize)
                            .map(|(_, s)| s),
                    }
                    .ok_or_else(|| anyhow::anyhow!("Session {} not found", session))?;
                    if session.connected.load(Ordering::Relaxed) {
                        return Err(anyhow::anyhow!("Session already connected"));
                    }
                    info!(target: &session.log_group(), "Attaching");
                    let size = if let Some(size) = size {
                        Size {
                            rows: size.rows as u16,
                            cols: size.cols as u16,
                            ..Size::term_size()?
                        }
                    } else {
                        Size::term_size()?
                    };
                    session.pty.resize(&Size {
                        cols: (size.cols as u16).checked_sub(2).unwrap_or(2),
                        rows: (size.rows as u16).checked_sub(2).unwrap_or(2),
                    })?;
                    tokio::task::spawn({
                        let sock_path = session.sock_path.clone();
                        let socket = session.listener.clone();
                        let file = session.pty.file().as_raw_fd();
                        let file = unsafe { libc::fcntl(file, libc::F_DUPFD, file) };
                        let connected = session.connected.clone();
                        async move {
                            Session::start(sock_path, socket, file, connected, size).await?;
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
                            info!(target: &session.log_group(), "Detaching");
                            session.detach().await?;
                            info!(target: &session.log_group(), "Detached");
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
                            info!(target: &session.log_group(), "Killing subprocess");
                            true
                        } else {
                            false
                        }
                    } else {
                        false
                    };
                    if sessions.is_empty() {
                        self.exit_signal.send(())?;
                    }
                    Ok(CommandResponse::KillSession(SeshKillResponse { success }))
                } else {
                    // TODO: Kill the *current* session and exit?
                    Ok(CommandResponse::KillSession(SeshKillResponse {
                        success: false,
                    }))
                }
            }
            Command::ShutdownServer => {
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
