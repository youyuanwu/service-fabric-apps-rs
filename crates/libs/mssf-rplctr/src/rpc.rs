use std::sync::Arc;

use bytes::Bytes;
use mssf_ext::traits::StateProvider;
use tonic::{Response, Status};

use crate::rplctr_inner::RplctrInner;

tonic::include_proto!("rplctr_rpc"); // The string specified here must match the proto package name

pub struct RpcService<T>
where
    T: StateProvider,
{
    inner: Arc<RplctrInner<T>>,
}

impl<T> RpcService<T>
where
    T: StateProvider,
{
    pub fn new(inner: Arc<RplctrInner<T>>) -> Self {
        Self { inner }
    }
}

#[tonic::async_trait]
impl<T> rplctr_service_server::RplctrService for RpcService<T>
where
    T: StateProvider,
{
    async fn replicate(
        &self,
        req: tonic::Request<RpcOperationRequest>,
    ) -> Result<tonic::Response<RpcOperationResponse>, tonic::Status> {
        let req = req.into_inner();
        let sn = req.sn;
        // pipe the data to the inner.
        self.inner
            .add_replicate_data(req.sn as i64, Bytes::from(req.data))
            .await
            .map_err(|e| Status::internal(format!("internal error {e}")))?;
        Ok(Response::new(RpcOperationResponse { sn, ack: true }))
    }
}
