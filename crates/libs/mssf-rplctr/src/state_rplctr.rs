// -- State replicator --

use std::sync::Arc;

use bytes::{Buf, BytesMut};
use mssf_core::{
    runtime::store_types::ReplicatorSettings,
    sync::{CancellationToken, FabricReceiver2},
};
use mssf_ext::traits::{Operation, OperationData, OperationStream, StateProvider, StateReplicator};

use crate::rplctr_inner::RplctrInner;

pub struct StRplctr<T: StateProvider> {
    _inner: Arc<RplctrInner<T>>,
}

impl<T: StateProvider> StRplctr<T> {
    pub fn new(inner: Arc<RplctrInner<T>>) -> Self {
        Self { _inner: inner }
    }
}

impl<T: StateProvider> StateReplicator for StRplctr<T> {
    // called on primary to send quorum data to secondary
    fn replicate(
        &self,
        _operation_data: impl OperationData,
        _: CancellationToken,
    ) -> (i64, FabricReceiver2<mssf_core::Result<i64>>) {
        todo!()
    }

    // called on secondary to get new data from primary
    fn get_replication_stream(&self) -> mssf_core::Result<impl OperationStream> {
        Ok(DummyOperationStream {})
    }

    // called on secondary to get copy data for catchup from primary
    fn get_copy_stream(&self) -> mssf_core::Result<impl OperationStream> {
        Ok(DummyOperationStream {})
    }

    fn update_replicator_settings(&self, _settings: &ReplicatorSettings) -> mssf_core::Result<()> {
        todo!()
    }

    fn get_replicator_settings(&self) -> mssf_core::Result<ReplicatorSettings> {
        todo!()
    }
}

pub struct DummyOperationStream {}

impl OperationStream for DummyOperationStream {
    async fn get_operation(
        &self,
        _: CancellationToken,
    ) -> mssf_core::Result<Option<impl Operation>> {
        Ok(Some(DummyOperation {}))
    }

    fn report_fault(&self) -> mssf_core::Result<()> {
        todo!()
    }
}

pub struct DummyOperation {}

impl Operation for DummyOperation {
    fn get_metadate(&self) -> mssf_ext::types::OperationMetadata {
        todo!()
    }

    fn get_data(&self) -> mssf_core::Result<impl Buf + Send> {
        Ok(BytesMut::from("dummy"))
    }

    fn acknowledge(&self) -> mssf_core::Result<()> {
        todo!()
    }
}
