use std::sync::Arc;

use bytes::{Buf, BytesMut};
use mssf_core::{
    runtime::{
        executor::DefaultExecutor,
        stateful::{PrimaryReplicator, Replicator},
        stateful_types::{Epoch, ReplicaInfo, ReplicaSetConfig, ReplicaSetQuarumMode},
        store_types::ReplicatorSettings,
    },
    types::ReplicaRole,
    HSTRING,
};
use mssf_ext::traits::{Operation, OperationData, OperationStream, StateProvider, StateReplicator};

pub struct RplctrInner<T: StateProvider> {
    state_prov: T,
    _rt: DefaultExecutor,
}

impl<T: StateProvider> RplctrInner<T> {
    fn new(state_prov: T, rt: DefaultExecutor) -> Self {
        Self {
            state_prov,
            _rt: rt,
        }
    }
}

pub struct StRplctr<T: StateProvider> {
    _inner: Arc<RplctrInner<T>>,
}

impl<T: StateProvider> StRplctr<T> {
    fn new(inner: Arc<RplctrInner<T>>) -> Self {
        Self { _inner: inner }
    }
}

pub struct Rplctr<T: StateProvider> {
    inner: Arc<RplctrInner<T>>,
}

impl<T: StateProvider> Rplctr<T> {
    pub fn new(state_prov: T, rt: DefaultExecutor) -> (Self, impl StateReplicator) {
        let inner = Arc::new(RplctrInner::new(state_prov, rt));
        let st_rplctr = StRplctr::new(inner.clone());
        (Self { inner }, st_rplctr)
    }
}

impl<T: StateProvider> Replicator for Rplctr<T> {
    async fn open(&self) -> mssf_core::Result<HSTRING> {
        // start rpc server
        todo!()
    }
    async fn close(&self) -> mssf_core::Result<()> {
        todo!()
    }
    async fn change_role(&self, _epoch: &Epoch, _role: &ReplicaRole) -> mssf_core::Result<()> {
        todo!()
    }
    // called only on secondaries.
    async fn update_epoch(&self, _epoch: &Epoch) -> mssf_core::Result<()> {
        todo!()
    }
    fn get_current_progress(&self) -> mssf_core::Result<i64> {
        todo!()
    }
    fn get_catch_up_capability(&self) -> mssf_core::Result<i64> {
        todo!()
    }
    fn abort(&self) {
        todo!()
    }
}

impl<T: StateProvider> PrimaryReplicator for Rplctr<T> {
    async fn on_data_loss(&self) -> mssf_core::Result<u8> {
        self.inner.state_prov.on_data_loss().await.map(|c| match c {
            true => 1,
            false => 0,
        })
    }
    fn update_catch_up_replica_set_configuration(
        &self,
        _currentconfiguration: &ReplicaSetConfig,
        _previousconfiguration: &ReplicaSetConfig,
    ) -> mssf_core::Result<()> {
        todo!()
    }
    async fn wait_for_catch_up_quorum(
        &self,
        _catchupmode: ReplicaSetQuarumMode,
    ) -> mssf_core::Result<()> {
        todo!()
    }
    fn update_current_replica_set_configuration(
        &self,
        _currentconfiguration: &ReplicaSetConfig,
    ) -> mssf_core::Result<()> {
        todo!()
    }
    async fn build_replica(&self, _replica: &ReplicaInfo) -> mssf_core::Result<()> {
        todo!()
    }
    fn remove_replica(&self, _replicaid: i64) -> mssf_core::Result<()> {
        todo!()
    }
}

impl<T: StateProvider> StateReplicator for StRplctr<T> {
    // called on primary to send quorum data to secondary
    async fn replicate(
        &self,
        _operation_data: impl OperationData,
        _sequence_number: &mut i64,
    ) -> mssf_core::Result<i64> {
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
    async fn get_operation(&self) -> mssf_core::Result<Option<impl Operation>> {
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
