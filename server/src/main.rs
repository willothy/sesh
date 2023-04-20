use anyhow::Result;
use sesh_shared::{pty::Pty, term::Size};
use std::{
    collections::HashMap,
    os::fd::{FromRawFd, RawFd},
    path::PathBuf,
    sync::Arc,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixListener,
    sync::Mutex,
};
use tokio_stream::wrappers::UnixListenerStream;

use tonic::{transport::Server as RPCServer, Request, Response, Status};

use sesh_proto::{
    sesh_server::{Sesh as RPCDefs, SeshServer},
    SeshKillRequest, SeshKillResponse, SeshListResponse, SeshStartRequest, SeshStartResponse,
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
}

impl Session {
    fn new(id: usize, name: String, program: String, pty: Pty, sock_path: PathBuf) -> Self {
        Self {
            id,
            name,
            program,
            pty,
            // socket: Arc::new(Mutex::new(socket)),
            sock_path,
        }
    }

    async fn start(sock_path: PathBuf, fd: RawFd) -> Result<()> {
        let socket = UnixListener::bind(sock_path)?;
        let (stream, _addr) = socket.accept().await?;

        let (mut r_socket, mut w_socket) = stream.into_split();

        // let w_socket = Arc::new(Mutex::new(w_socket));
        // let r_socket = Arc::new(Mutex::new(r_socket));

        tokio::task::spawn({
            // let pty = unsafe { File::from_raw_fd(fd) };
            let mut pty = unsafe { tokio::fs::File::from_raw_fd(fd) };
            // let socket = w_socket.clone();
            async move {
                loop {
                    // let Ok(mut socket) = socket.try_lock() else {
                    //     tokio::task::yield_now().await;
                    //     continue;
                    // };
                    // let Ok(mut pty) = pty.try_lock() else {
                    //     tokio::task::yield_now().await;
                    //     continue;
                    // };
                    // let pty_file = pty; //pty.file();
                    let mut i_packet = [0; 4096];

                    let i_count = pty.read(&mut i_packet).await?;
                    let read = &i_packet[..i_count];
                    w_socket.write_all(&read).await?;
                    w_socket.flush().await?;
                    let len = read.len();
                    if len != 0 {
                        println!("wrote to tty: {}", len);
                    }
                    tokio::task::yield_now().await;
                }
                #[allow(unreachable_code)]
                Result::<_, anyhow::Error>::Ok(())
            }
        });
        tokio::task::spawn({
            // let pty = pty.clone();
            let mut pty = unsafe { tokio::fs::File::from_raw_fd(fd) };
            // let socket = self.socket.clone();
            // let socket = socket.clone();
            async move {
                loop {
                    // let mut socket = socket.lock().await.accept().await?.0;
                    // let Ok(mut socket) = socket.try_lock() else {
                    //     tokio::task::yield_now().await;
                    //     continue;
                    // };
                    // let Ok(mut pty) = pty.try_lock() else {
                    //     tokio::task::yield_now().await;
                    //     continue;
                    // };
                    // let pty_file = pty.file();

                    let mut o_packet = [0; 4096];

                    let o_count = r_socket.read(&mut o_packet).await?;
                    let read = &o_packet[..o_count];
                    pty.write_all(&read).await?;
                    pty.flush().await?;
                    let len = read.len();
                    if len != 0 {
                        println!("read from tty: {}", len);
                    }
                    tokio::task::yield_now().await;
                }
                #[allow(unreachable_code)]
                Result::<_, anyhow::Error>::Ok(())
            }
        });
        // tokio::task::spawn({
        //     let pty = self.pty.clone();
        //     async move {
        //         loop {
        //             let pty = pty.lock().await;
        //             tokio::signal::unix::signal(SignalKind::window_change())?
        //                 .recv()
        //                 .await;
        //             pty.resize(&Size::term_size()?)
        //                 .map_err(|e| anyhow::anyhow!("{:?}", e))?;
        //         }
        //         #[allow(unreachable_code)]
        //         Result::<_, anyhow::Error>::Ok(())
        //     }
        // });
        Ok(())
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let sock_path = PathBuf::from(format!("/tmp/sesh/{}.sock", self.name));
        // get rid of the socket
        std::fs::remove_file(sock_path).ok();
    }
}

#[derive(Clone)]
struct Sesh {
    sessions: Arc<Mutex<HashMap<String, Session>>>,
    // do I need to queue events?
    // just uncomment the queue related things, it's working already though probably not optimal
    // queue: Arc<Mutex<VecDeque<Command>>>,
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
        request: Request<sesh_proto::SeshListRequest>,
    ) -> Result<Response<sesh_proto::SeshListResponse>, Status> {
        let req = request.into_inner();

        let res = self.exec(Command::ListSessions(req)).await;

        match res {
            Ok(CommandResponse::ListSessions(response)) => Ok(Response::new(response)),
            Ok(_) => Err(Status::internal("Unexpected response")),
            Err(e) => Err(Status::internal(format!("Error: {}", e))),
        }
    }
}

impl Sesh {
    fn new() -> Result<Self> {
        Ok(Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
            // queue: Arc::new(Mutex::new(VecDeque::new())),
        })
    }

    pub async fn exec(&self, cmd: Command) -> Result<CommandResponse> {
        println!("Executing command: {:?}", cmd);
        // let mut queue = loop {
        //     let Ok(queue) = self.queue.try_lock() else {
        //         tokio::task::yield_now().await;
        //         continue;
        //     };
        //     break queue;
        // };

        // while let Some(next) = queue.pop_front() {
        match cmd {
            Command::ListSessions(_) => {
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
            }) => {
                let pty = Pty::spawn(&program, args, &Size::term_size()?)
                    .map_err(|e| anyhow::anyhow!("{:?}", e))?;

                let mut sessions = self.sessions.lock().await;
                let nsessions = sessions.len();

                // TODO: Find a better way to auto-name sessions
                let name = PathBuf::from(&name)
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or(name.replace("/", "_"));

                let sesh_name = format!("{}-{}", name, nsessions);
                let sock_name = format!("/tmp/sesh/{}.sock", &sesh_name);

                println!("Starting session: {} on {}", sesh_name, sock_name);

                let session = Session::new(
                    nsessions,
                    sesh_name.clone(),
                    program,
                    pty,
                    PathBuf::from(&sock_name),
                );
                tokio::task::spawn({
                    let sock = session.sock_path.clone();
                    let fd = session.pty.fd();
                    async move {
                        Session::start(sock, fd).await?;
                        Result::<_, anyhow::Error>::Ok(())
                    }
                });

                sessions.insert(sesh_name, session);
                Ok(CommandResponse::StartSession(SeshStartResponse {
                    socket: sock_name,
                }))
            }
            Command::KillSession(request) => {
                if let Some(session) = request.session {
                    let mut sessions = self.sessions.lock().await;
                    match session {
                        sesh_proto::sesh_kill_request::Session::Name(name) => {
                            println!("Killing session: {:?}", name);
                            Ok(CommandResponse::KillSession(SeshKillResponse {
                                success: sessions.remove(&name).is_some(),
                            }))
                        }
                        sesh_proto::sesh_kill_request::Session::Id(id) => {
                            println!("Killing session: {:?}", id);
                            let mut success = false;
                            sessions.retain(|_, s| {
                                if s.id == id as usize {
                                    success = true;
                                    false
                                } else {
                                    true
                                }
                            });
                            Ok(CommandResponse::KillSession(SeshKillResponse { success }))
                        }
                    }
                } else {
                    Ok(CommandResponse::KillSession(SeshKillResponse {
                        success: false,
                    }))
                }
            }
        }
        // tokio::task::yield_now().await;
        // }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let sock_path = PathBuf::from(SERVER_SOCK);
    let sock_parent = sock_path.parent();
    if let Some(parent) = sock_parent {
        std::fs::create_dir_all(parent)?;
    }

    let uds = UnixListener::bind(SERVER_SOCK)?;
    let uds_stream = UnixListenerStream::new(uds);

    RPCServer::builder()
        .add_service(SeshServer::new(Sesh::new()?))
        // .serve_with_incoming(uds_stream)
        .serve_with_incoming_shutdown(uds_stream, async move {
            // Shutdown on sigint
            tokio::signal::ctrl_c().await.unwrap();
        })
        .await?;

    // remove socket on exit
    std::fs::remove_file(&sock_path)?;

    Ok(())
}
