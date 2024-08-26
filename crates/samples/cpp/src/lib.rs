use std::time::Duration;

use mssf_com::FabricTypes::FABRIC_E_TIMEOUT;
use mssf_core::{
    error::FabricErrorCode,
    runtime::executor::DefaultExecutor,
    sync::{BridgeContext3, CancellationToken},
};
use windows_core::{implement, Interface};
use FabricStrings::{
    IFabricStringsApiTable, IFabricStringsApiTable_Impl, IFabricStringsBytes,
    IFabricStringsBytes_Impl,
};

#[allow(non_snake_case)]
mod FabricCommon;
#[allow(non_snake_case)]
pub mod FabricStrings;

lazy_static::lazy_static! {
    static ref RT: tokio::runtime::Runtime = tokio::runtime::Builder::new_multi_thread().worker_threads(1).enable_time().build().unwrap();
}

// implementation of module
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

impl IFabricStringsBytes_Impl for StringImpl {
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
            _ = cancellation_token.cancelled() => { Err(FabricErrorCode::OperationCanceled.into()) }
            _ = tokio::time::sleep(sleep) => {
                if timeout < default_duration{
                    Err(mssf_core::error::FabricError::from(FABRIC_E_TIMEOUT).into())
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

impl IFabricStringsApiTable_Impl for ApiTableBridge {
    fn BeginConcatStrings(
        &self,
        str1: Option<&FabricStrings::IFabricStringsBytes>,
        str2: Option<&FabricStrings::IFabricStringsBytes>,
        timeoutmilliseconds: u32,
        callback: Option<&crate::FabricCommon::IFabricAsyncOperationCallback>,
    ) -> windows_core::Result<crate::FabricCommon::IFabricAsyncOperationContext> {
        let s1 = str1.unwrap().clone();
        let s2 = str2.unwrap().clone();
        let inner = self.inner.clone();
        let (ctx, token) = BridgeContext3::make(callback);
        ctx.spawn(&self.rt, async move {
            inner
                .concat_strings(
                    s1,
                    s2,
                    Duration::from_millis(timeoutmilliseconds as u64),
                    token,
                )
                .await
        })
    }

    fn EndConcatStrings(
        &self,
        context: Option<&crate::FabricCommon::IFabricAsyncOperationContext>,
    ) -> windows_core::Result<FabricStrings::IFabricStringsBytes> {
        BridgeContext3::result(context)?
    }
}
