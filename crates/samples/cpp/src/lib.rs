use std::time::Duration;

use mssf_core::{
    runtime::executor::DefaultExecutor,
    sync::{BridgeContext, CancellationToken},
    ErrorCode,
};
use windows_core::{implement, Interface};

#[allow(non_snake_case)]
pub mod FabricStrings;
pub use FabricStrings::Microsoft::ServiceFabric::FabricStrings::*;

lazy_static::lazy_static! {
    static ref RT: tokio::runtime::Runtime = tokio::runtime::Builder::new_multi_thread().worker_threads(1).enable_time().build().unwrap();
}

// implementation of module

/// # Safety
/// This api should be called from C/Cpp code.
///
/// Entry point of the library
#[no_mangle]
pub unsafe extern "system" fn GetFabricStringsApiTable(
    out: *mut *mut IFabricStringsApiTable,
) -> windows_core::HRESULT {
    if out.is_null() {
        // we do not handle errors for now.
        panic!("out is null");
    }

    match get_fabric_strings_api_table_internal() {
        Ok(com) => {
            let raw = com.into_raw();
            *out = raw as *mut IFabricStringsApiTable;
            windows_core::HRESULT::from(windows_core::Error::empty())
        }
        Err(e) => windows_core::HRESULT::from(e),
    }
}

fn get_fabric_strings_api_table_internal() -> windows_core::Result<IFabricStringsApiTable> {
    Ok(ApiTableBridge::new(DefaultExecutor::new(RT.handle().clone())).into())
}

pub struct StringProxy {
    com: IFabricStringsBytes,
}

impl StringProxy {
    fn new(com: IFabricStringsBytes) -> Self {
        Self { com }
    }
    fn get_bytes(&self) -> &[u8] {
        let mut buffer: *mut u8 = std::ptr::null_mut();
        let mut len: u32 = 0;
        unsafe {
            self.com
                .GetBytes(std::ptr::addr_of_mut!(buffer), std::ptr::addr_of_mut!(len))
        };
        assert!(!buffer.is_null());
        unsafe { std::slice::from_raw_parts(buffer, len as usize) }
    }
}

#[implement(IFabricStringsBytes)]
pub struct StringImpl {
    data: String,
}

impl StringImpl {
    fn new(data: String) -> Self {
        Self { data }
    }
}

impl IFabricStringsBytes_Impl for StringImpl_Impl {
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    fn GetBytes(&self, buffer: *mut *mut u8, buffersize: *mut u32) {
        assert!(!buffer.is_null());
        assert!(!buffersize.is_null());
        let len = self.data.len();
        let ptr = self.data.as_ptr();
        unsafe { *buffer = ptr as *mut u8 };
        unsafe { *buffersize = len as u32 };
    }
}

#[derive(Debug, Clone)]
pub struct ApiTable {}

impl ApiTable {
    async fn concat_strings(
        &self,
        str1: IFabricStringsBytes,
        str2: IFabricStringsBytes,
        timeout: Duration,
        cancellation_token: CancellationToken,
    ) -> mssf_core::Result<IFabricStringsBytes> {
        // operation slowness
        let default_duration = Duration::from_millis(50);

        let sleep = std::cmp::min(default_duration, timeout);

        tokio::select! {
            _ = cancellation_token.cancelled() => { Err(ErrorCode::E_ABORT.into()) }
            _ = tokio::time::sleep(sleep) => {
                if timeout < default_duration{
                    Err(ErrorCode::FABRIC_E_TIMEOUT.into())
                }else{
                    let s1 = StringProxy::new(str1);
                    let s2 = StringProxy::new(str2);
                    let out_str = String::from_utf8_lossy(s1.get_bytes()) + String::from_utf8_lossy(s2.get_bytes());
                    let out_com: IFabricStringsBytes = StringImpl::new(out_str.to_string()).into();
                    Ok(out_com)
                }
            }
        }
    }
}

#[implement(IFabricStringsApiTable)]
pub struct ApiTableBridge {
    inner: ApiTable,
    rt: DefaultExecutor,
}

impl ApiTableBridge {
    fn new(rt: DefaultExecutor) -> Self {
        Self {
            inner: ApiTable {},
            rt,
        }
    }
}

impl IFabricStringsApiTable_Impl for ApiTableBridge_Impl {
    fn BeginConcatStrings(
        &self,
        str1: windows_core::Ref<IFabricStringsBytes>,
        str2: windows_core::Ref<IFabricStringsBytes>,
        timeoutmilliseconds: u32,
        callback: windows_core::Ref<mssf_com::FabricCommon::IFabricAsyncOperationCallback>,
    ) -> windows_core::Result<mssf_com::FabricCommon::IFabricAsyncOperationContext> {
        let s1 = str1.unwrap().clone();
        let s2 = str2.unwrap().clone();
        let inner = self.inner.clone();
        let (ctx, token) = BridgeContext::make(callback);
        ctx.spawn(&self.rt, async move {
            inner
                .concat_strings(
                    s1,
                    s2,
                    Duration::from_millis(timeoutmilliseconds as u64),
                    token,
                )
                .await
                .map_err(windows_core::Error::from)
        })
    }

    fn EndConcatStrings(
        &self,
        context: windows_core::Ref<mssf_com::FabricCommon::IFabricAsyncOperationContext>,
    ) -> windows_core::Result<IFabricStringsBytes> {
        BridgeContext::result(context)?
    }
}
