use std::sync::Arc;

use mssf_com::{
    FabricCommon::{IFabricAsyncOperationCallback, IFabricAsyncOperationContext},
    FabricRuntime::{IFabricOperationDataStream, IFabricStateProvider, IFabricStateProvider_Impl},
    FabricTypes::FABRIC_EPOCH,
};
use mssf_core::{runtime::executor::Executor, sync::BridgeContext, types::Epoch};
use tracing::info;
use windows_core::implement;

use crate::{
    stream::{OpeartionDataStreamBridge, OperationDataStreamProxy},
    traits::StateProvider,
};

// given a state provider trait,
// wrap it to be IFabricStateProvider
#[implement(IFabricStateProvider)]
pub struct StateProviderBridge<T, E>
where
    T: StateProvider,
    E: Executor,
{
    inner: Arc<T>,
    rt: E,
}

impl<T, E> StateProviderBridge<T, E>
where
    T: StateProvider,
    E: Executor,
{
    pub fn new(inner: T, rt: E) -> Self {
        Self {
            inner: Arc::new(inner),
            rt,
        }
    }
}

impl<T: StateProvider, E: Executor> IFabricStateProvider_Impl for StateProviderBridge_Impl<T, E> {
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    fn BeginUpdateEpoch(
        &self,
        epoch: *const FABRIC_EPOCH,
        previousepochlastsequencenumber: i64,
        callback: windows_core::Ref<IFabricAsyncOperationCallback>,
    ) -> mssf_core::WinResult<IFabricAsyncOperationContext> {
        info!("StateProviderBridge::BeginUpdateEpoch");
        let epoch2 = Epoch::from(unsafe { epoch.as_ref().unwrap() });
        let inner = self.inner.clone();
        let (ctx, token) = BridgeContext::make(callback);
        ctx.spawn(&self.rt, async move {
            inner
                .update_epoch(&epoch2, previousepochlastsequencenumber, token)
                .await
                .map_err(mssf_core::WinError::from)
        })
    }

    fn EndUpdateEpoch(
        &self,
        context: windows_core::Ref<IFabricAsyncOperationContext>,
    ) -> windows_core::Result<()> {
        info!("StateProviderBridge::EndUpdateEpoch");
        BridgeContext::result(context)?
    }

    fn GetLastCommittedSequenceNumber(&self) -> mssf_core::WinResult<i64> {
        self.inner
            .get_last_committed_sequence_number()
            .map_err(mssf_core::WinError::from)
    }

    fn BeginOnDataLoss(
        &self,
        callback: windows_core::Ref<IFabricAsyncOperationCallback>,
    ) -> mssf_core::WinResult<IFabricAsyncOperationContext> {
        let inner = self.inner.clone();
        let (ctx, token) = BridgeContext::make(callback);
        ctx.spawn(&self.rt, async move {
            inner
                .on_data_loss(token)
                .await
                .map_err(mssf_core::WinError::from)
        })
    }

    fn EndOnDataLoss(
        &self,
        context: windows_core::Ref<IFabricAsyncOperationContext>,
    ) -> windows_core::Result<u8> {
        BridgeContext::result(context)?
    }

    fn GetCopyContext(&self) -> windows_core::Result<IFabricOperationDataStream> {
        let stream = self.inner.get_copy_context()?;
        let bridge = OpeartionDataStreamBridge::new(stream, self.rt.clone()).into();
        Ok(bridge)
    }

    fn GetCopyState(
        &self,
        uptosequencenumber: i64,
        copycontextstream: windows_core::Ref<IFabricOperationDataStream>,
    ) -> windows_core::Result<IFabricOperationDataStream> {
        let proxy = OperationDataStreamProxy::new(copycontextstream.unwrap().clone());
        let stream = self.inner.get_copy_state(uptosequencenumber, proxy)?;

        let bridge = OpeartionDataStreamBridge::new(stream, self.rt.clone()).into();
        Ok(bridge)
    }
}
