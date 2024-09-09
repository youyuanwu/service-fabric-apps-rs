use std::{cell::Cell, sync::Arc};

use mssf_core::{
    runtime::{
        executor::DefaultExecutor,
        stateful::{PrimaryReplicator, Replicator},
        stateful_types::{Epoch, ReplicaInfo, ReplicaSetConfig, ReplicaSetQuarumMode},
        store_types::ReplicatorSettings,
    },
    sync::CancellationToken,
    types::ReplicaRole,
    HSTRING,
};
use mssf_ext::traits::{StateProvider, StateReplicator};
use tokio::task::JoinHandle;

use crate::{rplctr_inner::RplctrInner, state_rplctr::StRplctr};

pub struct Rplctr<T: StateProvider> {
    inner: Arc<RplctrInner<T>>,
    close_token: std::sync::Mutex<Cell<Option<CancellationToken>>>,
    close_wait: std::sync::Mutex<Cell<Option<JoinHandle<()>>>>,
}

impl<T: StateProvider> Rplctr<T> {
    pub fn new(
        state_prov: T,
        rt: DefaultExecutor,
        settings: &ReplicatorSettings,
    ) -> (Self, impl StateReplicator) {
        // Create shared inner for rpcltr(this) and state replicator.
        let inner = Arc::new(RplctrInner::new(state_prov, rt, settings));
        let st_rplctr = StRplctr::new(inner.clone());
        (
            Self {
                inner,
                close_token: Default::default(),
                close_wait: Default::default(),
            },
            st_rplctr,
        )
    }
}

impl<T: StateProvider> Replicator for Rplctr<T> {
    async fn open(&self, _: CancellationToken) -> mssf_core::Result<HSTRING> {
        // start rpc server
        let close_token = CancellationToken::new();
        let close_token_cp = close_token.clone();
        let inner_cp = self.inner.clone();
        let close_handle = tokio::spawn(async move {
            inner_cp.serve(close_token_cp).await;
        });
        let prev = self.close_token.lock().unwrap().replace(Some(close_token));
        assert!(prev.is_none());
        let prev_h = self.close_wait.lock().unwrap().replace(Some(close_handle));
        assert!(prev_h.is_none());
        Ok(HSTRING::from(self.inner.get_addr()))
    }
    async fn close(&self, _: CancellationToken) -> mssf_core::Result<()> {
        // cancel background server
        let token = self
            .close_token
            .lock()
            .unwrap()
            .take()
            .expect("token not found");
        token.cancel();
        // wait for job to be down
        let handle = self
            .close_wait
            .lock()
            .unwrap()
            .take()
            .expect("job handle is not found");
        handle.await.unwrap();
        Ok(())
    }
    async fn change_role(
        &self,
        epoch: &Epoch,
        role: &ReplicaRole,
        _: CancellationToken,
    ) -> mssf_core::Result<()> {
        self.inner.change_role(role.clone(), epoch.clone()).await
    }
    // called only on secondaries.
    async fn update_epoch(&self, _epoch: &Epoch, _: CancellationToken) -> mssf_core::Result<()> {
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
    async fn on_data_loss(&self, cancellation_token: CancellationToken) -> mssf_core::Result<u8> {
        self.inner
            .get_state_prov()
            .on_data_loss(cancellation_token)
            .await
            .map(|c| match c {
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
        _: CancellationToken,
    ) -> mssf_core::Result<()> {
        todo!()
    }
    fn update_current_replica_set_configuration(
        &self,
        _currentconfiguration: &ReplicaSetConfig,
    ) -> mssf_core::Result<()> {
        todo!()
    }
    async fn build_replica(
        &self,
        _replica: &ReplicaInfo,
        _: CancellationToken,
    ) -> mssf_core::Result<()> {
        todo!()
    }
    fn remove_replica(&self, _replicaid: i64) -> mssf_core::Result<()> {
        todo!()
    }
}
