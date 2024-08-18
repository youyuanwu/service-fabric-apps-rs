use mssf_core::types::ReplicaRole;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tonic::{transport::Server, Status};

tonic::include_proto!("rplctr_rpc"); // The string specified here must match the proto package name

async fn start_rpc_server(addr: String, token: CancellationToken, svc: RplctrSvc) {
    let addr = addr.parse().unwrap();
    Server::builder()
        .add_service(rplctr_service_server::RplctrServiceServer::new(svc))
        .serve_with_shutdown(addr, async {
            token.cancelled().await;
            println!("Graceful shutdown tonic complete")
        })
        .await
        .unwrap();
}

// todo: based on role rejects requests.
pub struct RplctrSvc {
    rplc_mpsc: mpsc::Sender<DataRequest>,
    ack_mpsc: mpsc::Sender<i64>,
    role: ReplicaRole,
}

#[tonic::async_trait]
impl rplctr_service_server::RplctrService for RplctrSvc {
    // handle replication from primary.
    // success return means the msg is acked.
    async fn replicate(
        &self,
        request: tonic::Request<DataRequest>,
    ) -> Result<tonic::Response<ReplicateResponse>, Status> {
        if !matches!(
            self.role,
            ReplicaRole::ActiveSecondary | ReplicaRole::IdleSecondary
        ) {
            return Err(Status::invalid_argument("not secondary".to_string()));
        }
        // queue the data
        let data = request.into_inner();
        let sn = data.sn;
        self.rplc_mpsc
            .send(data)
            .await
            .map_err(|e| Status::internal(format!("failed to send mpsc {}", e)))?;
        let resp = ReplicateResponse { sn };
        Ok(tonic::Response::new(resp))
    }

    async fn ack(
        &self,
        request: tonic::Request<SnRequest>,
    ) -> Result<tonic::Response<EmptyPayload>, Status> {
        if !matches!(self.role, ReplicaRole::Primary) {
            return Err(Status::invalid_argument("not secondary".to_string()));
        }
        let sn = request.into_inner().sn;
        self.ack_mpsc
            .send(sn)
            .await
            .map_err(|e| Status::internal(format!("failed to send mpsc {}", e)))?;
        Ok(tonic::Response::new(EmptyPayload {}))
    }
}
