use anyhow::{Context, Result};
use log::{info, trace};
use sesh_shared::{error::CResult, pty::Pty, term::Size};
use std::{
    os::fd::{FromRawFd, RawFd},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicI64, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{UnixListener, UnixStream},
};
use tonic::transport::{Endpoint, Uri};
use tower::service_fn;

use sesh_proto::{sesh_cli_client::SeshCliClient, ClientDetachRequest};
pub struct Session {
    pub id: usize,
    pub name: String,
    pub program: String,
    pub pty: Pty,
    pub listener: Arc<UnixListener>,
    pub info: SessionInfo,
}

pub struct SessionInfo {
    pub start_time: i64,
    pub attach_time: Arc<AtomicI64>,
    connected: Arc<AtomicBool>,
    sock_path: PathBuf,
}

impl SessionInfo {
    pub fn new(sock_path: PathBuf) -> Self {
        Self {
            start_time: chrono::Local::now().timestamp_millis(),
            attach_time: Arc::new(AtomicI64::new(0)),
            connected: Arc::new(AtomicBool::new(false)),
            sock_path,
        }
    }

    pub fn connected(&self) -> Arc<AtomicBool> {
        self.connected.clone()
    }

    pub fn sock_path(&self) -> &PathBuf {
        &self.sock_path
    }
}

impl Session {
    pub fn new(
        id: usize,
        name: String,
        program: String,
        pty: Pty,
        sock_path: PathBuf,
    ) -> Result<Self> {
        Ok(Self {
            id,
            name,
            program,
            pty,
            listener: Arc::new(UnixListener::bind(&sock_path)?),
            info: SessionInfo::new(sock_path),
        })
    }

    pub fn log_group(&self) -> String {
        format!("{}: {}", self.id, self.name)
    }

    pub fn pid(&self) -> i32 {
        self.pty.pid()
    }

    pub async fn start(
        sock_path: PathBuf,
        socket: Arc<UnixListener>,
        fd: RawFd,
        connected: Arc<AtomicBool>,
        size: Size,
        attach_time: Arc<AtomicI64>,
    ) -> Result<()> {
        info!(target: "session", "Listening on {:?}", sock_path);
        let (stream, _addr) = socket.accept().await?;
        attach_time.store(chrono::Utc::now().timestamp_millis(), Ordering::Relaxed);
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

    pub async fn detach(&self) -> Result<()> {
        self.info.connected.store(false, Ordering::Relaxed);
        let parent = self
            .info
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
        std::fs::remove_file(&self.info.sock_path).ok();
    }
}
