// ------------------------------------------------------------
// Copyright 2024 Youyuan Wu
// Licensed under the MIT License (MIT). See License in the repo root for
// license information.
// ------------------------------------------------------------

use mssf_com::FabricCommon::{IFabricAsyncOperationCallback, IFabricAsyncOperationContext};
use mssf_core::sync::wait::AsyncContext;
use sfrc_c::ReliableCollectionRuntime::{IFabricDataLossHandler, IFabricDataLossHandler_Impl};
use windows_core::implement;

// dummy handler
#[derive(Debug)]
#[implement(IFabricDataLossHandler)]
pub struct DataLossHandler {}

impl IFabricDataLossHandler_Impl for DataLossHandler_Impl {
    fn BeginOnDataLoss(
        &self,
        callback: windows_core::Ref<IFabricAsyncOperationCallback>,
    ) -> mssf_core::WinResult<IFabricAsyncOperationContext> {
        let ctx: IFabricAsyncOperationContext = AsyncContext::new(callback.as_ref()).into();
        // TODO: maybe ctx return needs to set first
        unsafe { ctx.Callback().expect("cannot get callback").Invoke(&ctx) };
        Ok(ctx)
    }

    fn EndOnDataLoss(
        &self,
        _context: windows_core::Ref<IFabricAsyncOperationContext>,
        isstatechanged: *mut u8,
    ) -> mssf_core::WinResult<()> {
        unsafe { *isstatechanged = 0 };
        Ok(())
    }
}
