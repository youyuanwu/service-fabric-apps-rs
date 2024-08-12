use mssf_com::FabricRuntime::{IFabricOperationData, IFabricStateReplicator2};
use mssf_core::{runtime::store_types::ReplicatorSettings, sync::fabric_begin_end_proxy};

use crate::{
    data::OperationDataBridge,
    stream::OperationStreamProxy,
    traits::{OperationData, OperationStream, StateReplicator},
};

use windows_core::Interface;

#[derive(Clone)]
pub struct StateReplicatorProxy {
    com_impl: IFabricStateReplicator2,
}

impl StateReplicatorProxy {
    pub fn new(com_impl: IFabricStateReplicator2) -> Self {
        Self { com_impl }
    }
}

impl StateReplicator for StateReplicatorProxy {
    async fn replicate(
        &self,
        operation_data: impl OperationData,
        sequence_number: &mut i64,
    ) -> mssf_core::Result<i64> {
        let data_bridge: IFabricOperationData = OperationDataBridge::new(operation_data).into();
        let com1 = &self.com_impl;
        let com2 = self.com_impl.clone();
        let rx = fabric_begin_end_proxy(
            move |callback| unsafe { com1.BeginReplicate(&data_bridge, callback, sequence_number) },
            move |ctx| unsafe { com2.EndReplicate(ctx) },
        );
        rx.await
    }
    fn get_replication_stream(&self) -> mssf_core::Result<impl OperationStream> {
        let s = unsafe { self.com_impl.GetReplicationStream() }?;
        let proxy = OperationStreamProxy::new(s.cast().unwrap());
        Ok(proxy)
    }
    fn get_copy_stream(&self) -> mssf_core::Result<impl OperationStream> {
        let s = unsafe { self.com_impl.GetCopyStream() }?;
        let proxy = OperationStreamProxy::new(s.cast().unwrap());
        Ok(proxy)
    }
    fn update_replicator_settings(&self, settings: &ReplicatorSettings) -> mssf_core::Result<()> {
        let raw = settings.get_raw();
        unsafe { self.com_impl.UpdateReplicatorSettings(&raw) }
    }
    fn get_replicator_settings(&self) -> mssf_core::Result<ReplicatorSettings> {
        let raw = unsafe { self.com_impl.GetReplicatorSettings() }?;
        let _settings = unsafe { raw.get_ReplicatorSettings() };
        todo!(); // conversion not implemented
                 //Ok(ReplicatorSettings::from(settings))
    }
}
