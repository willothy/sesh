use log::error;
use sesh_proto::{
    seshd_server::Seshd as RPCDefs, SeshKillRequest, SeshKillResponse, SeshResizeRequest,
    SeshResizeResponse, SeshStartRequest, SeshStartResponse, ShutdownServerRequest,
    ShutdownServerResponse,
};
use tonic::{Request, Response, Status};

use crate::{
    commands::{Command, CommandResponse},
    Seshd,
};

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
