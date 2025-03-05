use std::sync::Arc;

use bytes::Bytes;
use mssf_core::{
    ErrorCode,
    runtime::executor::DefaultExecutor,
    sync::CancellationToken,
    types::{Epoch, ReplicaRole, ReplicatorSettings},
};
use mssf_ext::traits::{Operation, OperationStream, StateProvider};
use tokio::sync::mpsc;

use crate::rpc::RpcService;

/// Inner shared between replicator and stateprovider
pub struct RplctrInner<T: StateProvider> {
    state_prov: T,
    _rt: DefaultExecutor,
    addr: String,
    // secondary receives from primary
    sec_rplct_stream_rx: Arc<tokio::sync::Mutex<mpsc::Receiver<(i64, Bytes)>>>,
    sec_rplct_stream_tx: mpsc::Sender<(i64, Bytes)>,
}

impl<T: StateProvider> RplctrInner<T> {
    pub fn get_addr(&self) -> String {
        self.addr.clone()
    }

    pub fn get_state_prov(&self) -> &T {
        &self.state_prov
    }

    pub fn new(state_prov: T, rt: DefaultExecutor, settings: &ReplicatorSettings) -> Self {
        let (tx, rx) = mpsc::channel(100);
        Self {
            state_prov,
            _rt: rt,
            addr: settings.replicator_address.to_string(),
            sec_rplct_stream_rx: Arc::new(tokio::sync::Mutex::new(rx)),
            sec_rplct_stream_tx: tx,
        }
    }

    // called on secondary to get stream of data from primary
    pub fn get_replicatrion_stream(&self) -> ReplicationOperationStream {
        // TODO: assert on role
        ReplicationOperationStream {
            inner: self.sec_rplct_stream_rx.clone(),
        }
    }

    // add primary sent data here to secondary
    pub async fn add_replicate_data(&self, sn: i64, data: Bytes) -> mssf_core::Result<()> {
        self.sec_rplct_stream_tx
            .send((sn, data))
            .await
            .map_err(|_| ErrorCode::E_FAIL.into())
    }

    // open rpc server and block current task.
    pub async fn serve(self: Arc<Self>, token: CancellationToken) {
        let addr = self.addr.parse().unwrap();
        tonic::transport::Server::builder()
            .add_service(crate::rpc::rplctr_service_server::RplctrServiceServer::new(
                RpcService::new(self.clone()),
            ))
            .serve_with_shutdown(addr, async {
                token.cancelled().await;
                println!("Graceful shutdown tonic complete")
            })
            .await
            .unwrap();
    }

    pub async fn change_role(&self, _role: ReplicaRole, _epoch: Epoch) -> mssf_core::Result<()> {
        todo!()
    }
}

// -- replication stream
pub struct ReplicationOperationStream {
    inner: Arc<tokio::sync::Mutex<mpsc::Receiver<(i64, Bytes)>>>,
}

impl OperationStream for ReplicationOperationStream {
    async fn get_operation(
        &self,
        _: CancellationToken,
    ) -> mssf_core::Result<Option<impl Operation>> {
        let (sn, data) = self
            .inner
            .lock()
            .await
            .recv()
            .await
            .expect("cannot receive");
        let op = ReplicationOperation::new(data, sn);
        Ok(Some(op))
    }

    fn report_fault(&self) -> mssf_core::Result<()> {
        // not supported for now
        todo!()
    }
}

pub struct ReplicationOperation {
    data: Bytes,
    sn: i64,
}

impl ReplicationOperation {
    pub fn new(data: Bytes, sn: i64) -> Self {
        Self { data, sn }
    }
}

impl Operation for ReplicationOperation {
    fn get_metadate(&self) -> mssf_ext::types::OperationMetadata {
        mssf_ext::types::OperationMetadata {
            operation_type: mssf_ext::types::OperationType::Normal,
            sequence_number: self.sn,
            atomic_group_id: 0,
        }
    }

    fn get_data(&self) -> mssf_core::Result<impl bytes::Buf + Send> {
        Ok(self.data.clone())
    }

    fn acknowledge(&self) -> mssf_core::Result<()> {
        // assmume not ack for now.
        Ok(())
    }
}
