use std::sync::Arc;

use mssf_com::{
    FabricCommon::{IFabricAsyncOperationCallback, IFabricAsyncOperationContext},
    FabricRuntime::{IFabricOperationDataStream, IFabricStateProvider, IFabricStateProvider_Impl},
    FabricTypes::{FABRIC_EPOCH, FABRIC_SEQUENCE_NUMBER},
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
        previousepochlastsequencenumber: FABRIC_SEQUENCE_NUMBER,
        callback: windows_core::Ref<IFabricAsyncOperationCallback>,
    ) -> mssf_core::WinResult<IFabricAsyncOperationContext> {
        info!("StateProviderBridge::BeginUpdateEpoch");
        let epoch2 = Epoch::from(unsafe { epoch.as_ref().unwrap() });
        let inner = self.inner.clone();
        let (ctx, token) = BridgeContext::make(callback);
        ctx.spawn(&self.rt, async move {
            inner
                .update_epoch(&epoch2, previousepochlastsequencenumber.0, token)
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

    fn GetLastCommittedSequenceNumber(&self) -> mssf_core::WinResult<FABRIC_SEQUENCE_NUMBER> {
        self.inner
            .get_last_committed_sequence_number()
            .map(FABRIC_SEQUENCE_NUMBER)
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
    ) -> windows_core::Result<bool> {
        BridgeContext::result(context)?
    }

    fn GetCopyContext(&self) -> windows_core::Result<IFabricOperationDataStream> {
        let stream = self.inner.get_copy_context()?;
        let bridge = OpeartionDataStreamBridge::new(stream, self.rt.clone()).into();
        Ok(bridge)
    }

    fn GetCopyState(
        &self,
        uptosequencenumber: FABRIC_SEQUENCE_NUMBER,
        copycontextstream: windows_core::Ref<IFabricOperationDataStream>,
    ) -> windows_core::Result<IFabricOperationDataStream> {
        let proxy = OperationDataStreamProxy::new(copycontextstream.unwrap().clone());
        let stream = self.inner.get_copy_state(uptosequencenumber.0, proxy)?;

        let bridge = OpeartionDataStreamBridge::new(stream, self.rt.clone()).into();
        Ok(bridge)
    }
}

#[cfg(test)]
mod test {
    use bytes::Bytes;
    use mssf_core::{runtime::executor::BoxedCancelToken, sync::fabric_begin_end_proxy};
    use mssf_util::tokio::TokioExecutor;

    use crate::{
        data::OperationDataBuf,
        traits::{OperationData, OperationDataStream},
    };

    use super::*;

    struct EmptyDataStream;

    impl OperationDataStream for EmptyDataStream {
        async fn get_next(
            &self,
            _: BoxedCancelToken,
        ) -> mssf_core::Result<Option<impl OperationData>> {
            Ok(None::<OperationDataBuf<Bytes>>)
        }
    }

    struct TestStateProvider {
        state_changed: bool,
        sequence_number: i64,
    }

    impl StateProvider for TestStateProvider {
        async fn update_epoch(
            &self,
            _: &Epoch,
            previous_sequence_number: i64,
            _: BoxedCancelToken,
        ) -> mssf_core::Result<()> {
            assert_eq!(previous_sequence_number, self.sequence_number);
            Ok(())
        }

        fn get_last_committed_sequence_number(&self) -> mssf_core::Result<i64> {
            Ok(self.sequence_number)
        }

        async fn on_data_loss(&self, _: BoxedCancelToken) -> mssf_core::Result<bool> {
            Ok(self.state_changed)
        }

        fn get_copy_context(&self) -> mssf_core::Result<impl OperationDataStream> {
            Ok(EmptyDataStream)
        }

        fn get_copy_state(
            &self,
            upto_sequence_number: i64,
            _: impl OperationDataStream,
        ) -> mssf_core::Result<impl OperationDataStream> {
            assert_eq!(upto_sequence_number, self.sequence_number);
            Ok(EmptyDataStream)
        }
    }

    fn create_provider(state_changed: bool, sequence_number: i64) -> IFabricStateProvider {
        StateProviderBridge::new(
            TestStateProvider {
                state_changed,
                sequence_number,
            },
            TokioExecutor::new(tokio::runtime::Handle::current()),
        )
        .into()
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn data_loss_preserves_boolean_result() {
        for state_changed in [false, true] {
            let provider = create_provider(state_changed, 0);
            let begin = provider.clone();
            let result = fabric_begin_end_proxy(
                move |callback| unsafe { begin.BeginOnDataLoss(callback) },
                move |context| unsafe { provider.EndOnDataLoss(context) },
                None,
            );
            assert_eq!(result.await.unwrap().unwrap(), state_changed);
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn state_provider_preserves_sequence_numbers() {
        for sequence_number in [i64::MIN, i64::MAX] {
            let provider = create_provider(false, sequence_number);
            assert_eq!(
                unsafe { provider.GetLastCommittedSequenceNumber() }.unwrap(),
                FABRIC_SEQUENCE_NUMBER(sequence_number)
            );

            let epoch = FABRIC_EPOCH::default();
            let begin = provider.clone();
            let end = provider.clone();
            let result = fabric_begin_end_proxy(
                move |callback| unsafe {
                    begin.BeginUpdateEpoch(
                        &epoch,
                        FABRIC_SEQUENCE_NUMBER(sequence_number),
                        callback,
                    )
                },
                move |context| unsafe { end.EndUpdateEpoch(context) }.ok(),
                None,
            );
            result.await.unwrap().unwrap();

            let copy_context = unsafe { provider.GetCopyContext() }.unwrap();
            unsafe {
                provider.GetCopyState(FABRIC_SEQUENCE_NUMBER(sequence_number), &copy_context)
            }
            .unwrap();
        }
    }
}
