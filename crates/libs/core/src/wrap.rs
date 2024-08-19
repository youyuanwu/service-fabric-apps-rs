// ------------------------------------------------------------
// Copyright (c) Microsoft Corporation.  All rights reserved.
// Licensed under the MIT License (MIT). See License in the repo root for
// license information.
// ------------------------------------------------------------

use core::slice;
use std::{ffi::c_void, ptr::addr_of_mut};

use mssf_com::FabricRuntime::{IFabricPrimaryReplicator, IFabricStatefulServicePartition};
use sfrc_c::ReliableCollectionRuntime::{
    fnNotifyAsyncCompletion, fnNotifyCreateEnumeratorAsyncCompletion, fnNotifyGetAsyncCompletion,
    fnNotifyGetOrAddStateProviderAsyncCompletion, fnNotifyRemoveAsyncCompletion,
    fnNotifyStoreKeyValueEnumeratorMoveNextAsyncCompletion, Buffer, Buffer_Release,
    CancellationTokenSource_Cancel, CancellationTokenSource_Release, GetTxnReplicator,
    IFabricDataLossHandler, ReliableCollectionRuntime_Initialize,
    ReliableCollectionRuntime_Initialize2, StateProvider_Info,
    StoreKeyValueEnumerator_MoveNextAsync, StoreKeyValueEnumerator_Release, Store_AddAsync,
    Store_ConditionalGetAsync, Store_ConditionalRemoveAsync, Store_CreateEnumeratorAsync,
    Store_GetCount, Store_LockMode, Store_Release, Transaction_Abort, Transaction_AddRef,
    Transaction_CommitAsync, Transaction_Dispose, Transaction_Release,
    TxnReplicator_CreateTransaction, TxnReplicator_GetInfo,
    TxnReplicator_GetOrAddStateProviderAsync, TxnReplicator_Info, TxnReplicator_Release,
    TxnReplicator_Settings, RELIABLECOLLECTION_API_VERSION,
};
use tokio::sync::oneshot::{self, Receiver, Sender};
use windows::{
    core::{Error, Interface, HRESULT, HSTRING, PCWSTR},
    Win32::Foundation::BOOL,
};

// do module init
pub struct ReliableCollectionRuntime {}

impl ReliableCollectionRuntime {
    pub fn create() -> ReliableCollectionRuntime {
        let ok =
            unsafe { ReliableCollectionRuntime_Initialize(RELIABLECOLLECTION_API_VERSION as u16) };
        ok.expect("cannot init");
        ReliableCollectionRuntime {}
    }

    pub fn create_standalone() -> ReliableCollectionRuntime {
        let ok = unsafe {
            ReliableCollectionRuntime_Initialize2(
                RELIABLECOLLECTION_API_VERSION.try_into().unwrap(),
                true,
            )
        };
        ok.expect("cannot init standalone");
        ReliableCollectionRuntime {}
    }
}

// proxy for the txn replicator handle
pub struct TxnReplicaReplicator {
    h: *const std::ffi::c_void,
}

unsafe impl Send for TxnReplicaReplicator {}
unsafe impl Sync for TxnReplicaReplicator {}

impl TxnReplicaReplicator {
    pub fn txn_replicator_get_info(&self) -> Result<TxnReplicator_Info, Error> {
        let mut info = TxnReplicator_Info {
            ..Default::default()
        };
        unsafe { TxnReplicator_GetInfo(self.h, std::ptr::addr_of_mut!(info))? };
        Ok(info)
    }

    pub fn create_transaction(&self) -> Result<Txn, Error> {
        let mut tx_handle = std::ptr::null_mut::<c_void>();
        unsafe { TxnReplicator_CreateTransaction(self.h, std::ptr::addr_of_mut!(tx_handle))? };
        assert!(!tx_handle.is_null());
        Ok(Txn { h: tx_handle })
    }

    pub fn release(self) {
        unsafe { TxnReplicator_Release(self.h) }
    }

    unsafe extern "system" fn get_or_add_state_provider_callback(
        ctx: *mut ::core::ffi::c_void,
        status: ::windows::core::HRESULT,
        // already got from front end
        store: *mut ::core::ffi::c_void,
        exist: ::windows::Win32::Foundation::BOOL,
    ) {
        let ctx_back =
            unsafe { Box::from_raw(ctx as *mut Sender<Result<(StateProvider, BOOL), Error>>) };

        let err = Error::from(status);
        if err.code() != HRESULT::default() {
            // failed
            let ok = ctx_back.send(Err(err));
            debug_assert!(ok.is_ok(), "frontend dropped");
            return;
        }

        let store = StateProvider { h: store };

        // send always success because receiver is present
        let ok = ctx_back.send(Ok((store, exist)));
        debug_assert!(ok.is_ok(), "frontend dropped");
    }

    // TxnReplicator_GetOrAddStateProviderAsync
    pub fn get_or_add_state_provider_async(
        &self,
        txn: &Txn,
        name: &HSTRING,
        lang: &HSTRING,
        stateproviderinfo: &StateProvider_Info,
        timeout: i64,
    ) -> Receiver<Result<(StateProvider, BOOL), Error>>
// provider, already exist
    {
        // cancellation
        let mut cts = CancellationToken::default();
        let mut stateprovider = std::ptr::null_mut::<c_void>();
        let mut alreadyexists = BOOL::default();
        let callback: fnNotifyGetOrAddStateProviderAsyncCompletion =
            Some(Self::get_or_add_state_provider_callback);
        let mut synchronouscomplete = BOOL::default();

        let (tx, rx) = oneshot::channel();

        // prepare ctx
        let ctx = Box::new(tx);
        let ctx_raw = &*ctx as *const Sender<Result<(StateProvider, BOOL), Error>>;
        std::mem::forget(ctx); // forget in front end.

        let ok = unsafe {
            TxnReplicator_GetOrAddStateProviderAsync(
                self.h,
                txn.h,
                PCWSTR(name.as_ptr()),
                PCWSTR(lang.as_ptr()),
                stateproviderinfo as *const StateProvider_Info,
                timeout,
                cts.init_void_addr(),
                std::ptr::addr_of_mut!(stateprovider),
                std::ptr::addr_of_mut!(alreadyexists),
                callback,
                ctx_raw as *const c_void,
                std::ptr::addr_of_mut!(synchronouscomplete),
            )
        };

        if ok.is_err() {
            // need to send the error back
            let ctx_back = unsafe {
                Box::from_raw(ctx_raw as *mut Sender<Result<(StateProvider, BOOL), Error>>)
            };
            ctx_back.send(Err(ok.unwrap_err())).unwrap();
        } else if synchronouscomplete.as_bool() {
            let store = StateProvider { h: stateprovider };
            // ctx is not used by backend
            let ctx_back = unsafe {
                Box::from_raw(ctx_raw as *mut Sender<Result<(StateProvider, BOOL), Error>>)
            };
            // send always success because receiver is present
            ctx_back.send(Ok((store, alreadyexists))).unwrap();
        }
        rx
    }
}

pub fn get_txn_replicator(
    replicaid: i64,
    partition: &IFabricStatefulServicePartition,
    dataloss_handler: &IFabricDataLossHandler,
    replicator_settings: &TxnReplicator_Settings,
    config_package_name: &HSTRING,
    replicator_settings_section_name: &HSTRING,
    replicator_security_section_name: &HSTRING,
) -> Result<(IFabricPrimaryReplicator, TxnReplicaReplicator), Error> {
    let mut replicator_raw: *mut std::ffi::c_void = std::ptr::null_mut();
    let mut txn_replicator_raw: *mut std::ffi::c_void = std::ptr::null_mut();
    unsafe {
        GetTxnReplicator(
            replicaid,
            partition.as_raw(),
            dataloss_handler.as_raw(),
            replicator_settings,
            PCWSTR(config_package_name.as_ptr()),
            PCWSTR(replicator_settings_section_name.as_ptr()),
            PCWSTR(replicator_security_section_name.as_ptr()),
            std::ptr::addr_of_mut!(replicator_raw),
            std::ptr::addr_of_mut!(txn_replicator_raw),
        )?
    };
    assert!(!replicator_raw.is_null());
    assert!(!txn_replicator_raw.is_null());

    let replicator = unsafe { IFabricPrimaryReplicator::from_raw(replicator_raw) };
    let txn_replicator = TxnReplicaReplicator {
        h: txn_replicator_raw,
    };
    Ok((replicator, txn_replicator))
}

pub struct CancellationToken {
    h: *mut std::ffi::c_void,
}

impl CancellationToken {
    pub fn cancel(&self) {
        unsafe { CancellationTokenSource_Cancel(self.h) };
    }

    // should only be called once.
    fn release(&self) {
        unsafe { CancellationTokenSource_Release(self.h) };
    }

    fn init_void_addr(&mut self) -> *mut *mut std::ffi::c_void {
        std::ptr::addr_of_mut!(self.h)
    }
}

impl Drop for CancellationToken {
    fn drop(&mut self) {
        if !self.h.is_null() {
            self.release();
            self.h = std::ptr::null_mut();
        }
    }
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self {
            h: std::ptr::null_mut(),
        }
    }
}

// buffer wrapper
#[derive(Default)]
struct TempBuffer {
    b: Buffer,
}

impl TempBuffer {
    // do not use
    pub fn release(&self) {
        assert!(!self.b.Handle.is_null());
        unsafe { Buffer_Release(self.b.Handle) };
    }

    pub fn init_void_addr(&mut self) -> *mut Buffer {
        std::ptr::addr_of_mut!(self.b)
    }

    pub fn to_vec(&self) -> Vec<u8> {
        Self::raw_to_vec(self.b.Bytes.0, self.b.Length as usize)
    }

    fn raw_to_vec(data: *const u8, len: usize) -> Vec<u8> {
        let val_view = unsafe { slice::from_raw_parts(data, len) };
        val_view.into()
    }
}

impl Drop for TempBuffer {
    fn drop(&mut self) {
        if !self.b.Handle.is_null() {
            self.release();
            self.b.Handle = std::ptr::null_mut();
        }
    }
}

#[derive(Debug)]
pub struct StateProvider {
    h: *mut std::ffi::c_void,
}

unsafe impl Send for StateProvider {}
unsafe impl Sync for StateProvider {}

impl StateProvider {
    // store api

    // should only be called once.
    // auto drop calls this
    pub fn release(&mut self) {
        assert!(!self.h.is_null());
        unsafe { Store_Release(self.h) };
        self.h = std::ptr::null_mut();
    }

    pub fn get_count(&self) -> Result<i64, Error> {
        unsafe { Store_GetCount(self.h) }
    }

    unsafe extern "system" fn conditional_get_async_callback(
        ctx: *mut ::core::ffi::c_void,
        status: HRESULT,
        found: ::windows::Win32::Foundation::BOOL,
        _objecthandle: usize,
        bytes: *mut ::core::ffi::c_void,
        byteslength: u32,
        versionsequencenumber: i64,
    ) {
        let ctx_back =
            unsafe { Box::from_raw(ctx as *mut Sender<Result<(BOOL, Vec<u8>, i64), Error>>) };
        if status.is_err() {
            let e = Error::from(status);
            let ok = ctx_back.send(Err(e));
            if ok.is_err() {
                debug_assert!(false, "front end dropped");
            }
            return;
        }

        let mut val = Vec::<u8>::default();
        if found.as_bool() {
            let val_view = unsafe { slice::from_raw_parts(bytes as *mut u8, byteslength as usize) };
            val = val_view.into();
        }
        ctx_back
            .send(Ok((found, val, versionsequencenumber)))
            .unwrap();
    }

    // object handle is a number stored along side the kv pair
    // serves as a unique identifier. It might not be enabled in sf, so we ignore it for now.
    #[allow(clippy::type_complexity)]
    pub fn conditional_get_async(
        &self,
        txn: &Txn,
        key: &HSTRING,
        timeout: i64,
        lockmode: Store_LockMode,
        // found, value, version sequence number
    ) -> Receiver<Result<(BOOL, Vec<u8>, i64), Error>> {
        let mut object_handle = usize::default();
        let mut buff = TempBuffer::default();
        let mut version_sequence_number = i64::default();
        let mut cts = CancellationToken::default();
        let mut found = BOOL::default();
        let callback: fnNotifyGetAsyncCompletion = Some(Self::conditional_get_async_callback);
        let mut synchronouscomplete = BOOL::default();

        let (tx, rx) = oneshot::channel();

        // prepare ctx
        let ctx = Box::new(tx);
        let ctx_raw = &*ctx as *const Sender<Result<(BOOL, Vec<u8>, i64), Error>>;
        std::mem::forget(ctx); // forget in front end.

        let ok = unsafe {
            Store_ConditionalGetAsync(
                self.h,
                txn.h,
                PCWSTR(key.as_ptr()),
                timeout,
                lockmode,
                std::ptr::addr_of_mut!(object_handle),
                std::ptr::addr_of_mut!(buff.b),
                std::ptr::addr_of_mut!(version_sequence_number),
                cts.init_void_addr(),
                std::ptr::addr_of_mut!(found),
                callback,
                ctx_raw as *const c_void,
                std::ptr::addr_of_mut!(synchronouscomplete),
            )
        };

        match ok {
            Ok(_) => {
                if synchronouscomplete.as_bool() {
                    let ctx_back = unsafe {
                        Box::from_raw(ctx_raw as *mut Sender<Result<(BOOL, Vec<u8>, i64), Error>>)
                    };
                    let mut val = Vec::<u8>::default();
                    if found.as_bool() {
                        val = buff.to_vec();
                    }
                    ctx_back
                        .send(Ok((found, val, version_sequence_number)))
                        .unwrap();
                }
            }
            Err(e) => {
                let ctx_back = unsafe {
                    Box::from_raw(ctx_raw as *mut Sender<Result<(BOOL, Vec<u8>, i64), Error>>)
                };
                ctx_back.send(Err(e)).unwrap();
            }
        }
        rx
    }

    unsafe extern "system" fn add_async_callback(ctx: *mut ::core::ffi::c_void, status: HRESULT) {
        let ctx_back = unsafe { Box::from_raw(ctx as *mut Sender<Result<(), Error>>) };
        let ok = if status.is_ok() {
            ctx_back.send(Ok(()))
        } else {
            ctx_back.send(Err(Error::from(status)))
        };

        if ok.is_err() {
            debug_assert!(false, "frontend dropped");
        }
    }

    pub fn add_async(
        &self,
        txn: &Txn,
        key: &HSTRING,
        val: &[u8],
        timeout: i64,
    ) -> Receiver<Result<(), Error>> {
        let object_handle = usize::default();
        let mut cts = CancellationToken::default();
        let callback: fnNotifyAsyncCompletion = Some(Self::add_async_callback);
        let mut synchronouscomplete = BOOL::default();

        let (tx, rx) = oneshot::channel();

        // prepare ctx
        let ctx = Box::new(tx);
        let ctx_raw = &*ctx as *const Sender<Result<(), Error>>;
        std::mem::forget(ctx); // forget in front end.

        let ok = unsafe {
            Store_AddAsync(
                self.h,
                txn.h,
                key,
                object_handle,
                val.as_ptr() as *const c_void,
                val.len() as u32,
                timeout,
                cts.init_void_addr(),
                callback,
                ctx_raw as *const c_void,
                std::ptr::addr_of_mut!(synchronouscomplete),
            )
        };

        match ok {
            Ok(_) => {
                if synchronouscomplete.as_bool() {
                    let ctx_back =
                        unsafe { Box::from_raw(ctx_raw as *mut Sender<Result<(), Error>>) };
                    ctx_back.send(Ok(())).unwrap();
                }
            }
            Err(e) => {
                let ctx_back = unsafe { Box::from_raw(ctx_raw as *mut Sender<Result<(), Error>>) };
                ctx_back.send(Err(e)).unwrap();
            }
        };
        rx
    }

    unsafe extern "system" fn conditional_remove_async_callback(
        ctx: *mut ::core::ffi::c_void,
        status: HRESULT,
        removed: ::windows::Win32::Foundation::BOOL,
    ) {
        let ctx_back = unsafe { Box::from_raw(ctx as *mut Sender<Result<BOOL, Error>>) };
        let ok = if status.is_ok() {
            ctx_back.send(Ok(removed))
        } else {
            ctx_back.send(Err(Error::from(status)))
        };
        debug_assert!(ok.is_ok(), "front end dropped");
    }

    pub fn conditional_remove_async(
        &self,
        txn: &Txn,
        key: &HSTRING,

        timeout: i64,
        conditionalversion: i64,
    ) -> Receiver<Result<BOOL, Error>> {
        let mut cts = CancellationToken::default();
        let mut removed = BOOL::default();
        let mut synchronouscomplete = BOOL::default();

        let callback: fnNotifyRemoveAsyncCompletion = Some(Self::conditional_remove_async_callback);
        let (tx, rx) = oneshot::channel();

        // prepare ctx
        let ctx = Box::new(tx);
        let ctx_raw = &*ctx as *const Sender<Result<BOOL, Error>>;
        std::mem::forget(ctx); // forget in front end.

        let ok = unsafe {
            Store_ConditionalRemoveAsync(
                self.h,
                txn.h,
                key,
                timeout,
                cts.init_void_addr(),
                conditionalversion,
                std::ptr::addr_of_mut!(removed),
                callback,
                ctx_raw as *const c_void,
                std::ptr::addr_of_mut!(synchronouscomplete),
            )
        };

        match ok {
            Ok(_) => {
                if synchronouscomplete.as_bool() {
                    let ctx_back =
                        unsafe { Box::from_raw(ctx_raw as *mut Sender<Result<BOOL, Error>>) };
                    ctx_back.send(Ok(removed)).unwrap();
                }
            }
            Err(e) => {
                let ctx_back =
                    unsafe { Box::from_raw(ctx_raw as *mut Sender<Result<BOOL, Error>>) };
                ctx_back.send(Err(e)).unwrap();
            }
        };
        rx
    }

    unsafe extern "system" fn create_enumerator_async_callback(
        ctx: *mut ::core::ffi::c_void,
        status: HRESULT,
        enumerator: *mut ::core::ffi::c_void,
    ) {
        let ctx_back =
            unsafe { Box::from_raw(ctx as *mut Sender<Result<KeyValueEnumerator, Error>>) };

        let ok = if status.is_err() {
            ctx_back.send(Err(Error::from(status)))
        } else {
            let enu = KeyValueEnumerator { h: enumerator };
            ctx_back.send(Ok(enu))
        };
        if ok.is_err() {
            debug_assert!(false, "frontend dropped");
        }
    }

    pub fn create_enumerator_async(
        &self,
        txn: &Txn,
    ) -> Receiver<Result<KeyValueEnumerator, Error>> {
        let mut enu = KeyValueEnumerator::default();
        let mut synchronouscomplete = BOOL::default();

        let callback: fnNotifyCreateEnumeratorAsyncCompletion =
            Some(Self::create_enumerator_async_callback);
        let (tx, rx) = oneshot::channel();

        // prepare ctx
        let ctx = Box::new(tx);
        let ctx_raw = &*ctx as *const Sender<Result<KeyValueEnumerator, Error>>;
        std::mem::forget(ctx); // forget in front end.

        let ok = unsafe {
            Store_CreateEnumeratorAsync(
                self.h,
                txn.h,
                enu.init_void_addr(),
                callback,
                ctx_raw as *const c_void,
                addr_of_mut!(synchronouscomplete),
            )
        };

        match ok {
            Ok(_) => {
                if synchronouscomplete.as_bool() {
                    let ctx_back = unsafe {
                        Box::from_raw(ctx_raw as *mut Sender<Result<KeyValueEnumerator, Error>>)
                    };
                    ctx_back.send(Ok(enu)).unwrap();
                }
            }
            Err(e) => {
                let ctx_back = unsafe {
                    Box::from_raw(ctx_raw as *mut Sender<Result<KeyValueEnumerator, Error>>)
                };
                ctx_back.send(Err(e)).unwrap();
            }
        };
        rx
    }
}

impl Drop for StateProvider {
    fn drop(&mut self) {
        if !self.h.is_null() {
            self.release()
        }
    }
}

pub struct Txn {
    h: *const std::ffi::c_void,
}

unsafe impl Send for Txn {}

impl Txn {
    pub fn release(&mut self) {
        assert!(!self.h.is_null());
        unsafe { Transaction_Release(self.h) };
        self.h = std::ptr::null();
    }

    pub fn dispose(&self) {
        unsafe { Transaction_Dispose(self.h) };
    }

    // do not use in rust
    pub fn add_ref(&self) {
        unsafe { Transaction_AddRef(self.h) };
    }

    pub fn abort(&self) -> Result<(), Error> {
        unsafe { Transaction_Abort(self.h) }
    }

    unsafe extern "system" fn commit_async_callback(
        ctx: *mut ::core::ffi::c_void,
        status: HRESULT,
    ) {
        let ctx_back = unsafe { Box::from_raw(ctx as *mut Sender<Result<(), Error>>) };
        let ok = if status.is_ok() {
            ctx_back.send(Ok(()))
        } else {
            ctx_back.send(Err(Error::from(status)))
        };
        if ok.is_err() {
            debug_assert!(false, "frontend dropped.")
        }
    }

    pub fn commit_async(&self) -> Receiver<Result<(), Error>> {
        let (tx, rx) = oneshot::channel();
        let ctx = Box::new(tx);
        let ctx_raw = &*ctx as *const Sender<Result<(), Error>>;
        std::mem::forget(ctx); // forget in front end.

        let callback: fnNotifyAsyncCompletion = Some(Self::commit_async_callback);

        let ok = unsafe { Transaction_CommitAsync(self.h, callback, ctx_raw as *const c_void) };
        match ok {
            Ok(synccomplete) => {
                if synccomplete.as_bool() {
                    // no callback will be invoked
                    let ctx_back =
                        unsafe { Box::from_raw(ctx_raw as *mut Sender<Result<(), Error>>) };
                    ctx_back.send(Ok(())).unwrap();
                }
            }
            Err(e) => {
                // no callback will be invoked
                let ctx_back = unsafe { Box::from_raw(ctx_raw as *mut Sender<Result<(), Error>>) };
                ctx_back.send(Err(e)).unwrap();
            }
        }
        rx
    }
}

impl Drop for Txn {
    fn drop(&mut self) {
        if !self.h.is_null() {
            self.dispose();
            self.release()
        }
    }
}

// enumerator
#[derive(Debug)]
pub struct KeyValueEnumerator {
    h: *mut std::ffi::c_void,
}

unsafe impl Send for KeyValueEnumerator {}

impl KeyValueEnumerator {
    fn init_void_addr(&mut self) -> *mut *mut std::ffi::c_void {
        std::ptr::addr_of_mut!(self.h)
    }

    pub fn release(&mut self) {
        assert!(!self.h.is_null());
        unsafe { StoreKeyValueEnumerator_Release(self.h) };
        self.h = std::ptr::null_mut();
    }

    unsafe extern "system" fn move_next_async_callback(
        ctx: *mut ::core::ffi::c_void,
        status: HRESULT,
        advanced: ::windows::Win32::Foundation::BOOL,
        key: PCWSTR,
        _objecthandle: usize,
        bytebuffer: *mut ::core::ffi::c_void,
        bufferlength: u32,
        versionsequencenumber: i64,
    ) {
        let ctx_back = unsafe {
            Box::from_raw(ctx as *mut Sender<Result<(BOOL, HSTRING, Vec<u8>, i64), Error>>)
        };

        let ok = if status.is_err() {
            ctx_back.send(Err(Error::from(status)))
        } else {
            ctx_back.send(Ok((
                advanced,
                HSTRING::from_wide(unsafe { key.as_wide() }).unwrap(),
                TempBuffer::raw_to_vec(bytebuffer as *const u8, bufferlength as usize),
                versionsequencenumber,
            )))
        };
        if ok.is_err() {
            debug_assert!(false, "frontend dropped");
        }
    }

    // advanced, key, val, vsn
    #[allow(clippy::type_complexity)]
    pub fn move_next_async(&self) -> Receiver<Result<(BOOL, HSTRING, Vec<u8>, i64), Error>> {
        let mut cts = CancellationToken::default();
        let mut advanced = BOOL::default();
        let mut key = PCWSTR::null();
        let mut objecthandle = usize::default();
        let mut buff = TempBuffer::default();
        let mut versionsequencenumber = i64::default();
        let callback: fnNotifyStoreKeyValueEnumeratorMoveNextAsyncCompletion =
            Some(Self::move_next_async_callback);

        let (tx, rx) = oneshot::channel();
        let ctx = Box::new(tx);
        let ctx_raw = &*ctx as *const Sender<Result<(BOOL, HSTRING, Vec<u8>, i64), Error>>;
        std::mem::forget(ctx); // forget in front end.
        let mut synchronouscomplete = BOOL::default();

        let ok = unsafe {
            StoreKeyValueEnumerator_MoveNextAsync(
                self.h,
                cts.init_void_addr(),
                std::ptr::addr_of_mut!(advanced),
                std::ptr::addr_of_mut!(key),
                std::ptr::addr_of_mut!(objecthandle),
                buff.init_void_addr(),
                std::ptr::addr_of_mut!(versionsequencenumber),
                callback,
                ctx_raw as *const c_void,
                std::ptr::addr_of_mut!(synchronouscomplete),
            )
        };

        match ok {
            Ok(()) => {
                if synchronouscomplete.as_bool() {
                    // no callback will be invoked
                    let ctx_back = unsafe {
                        Box::from_raw(
                            ctx_raw as *mut Sender<Result<(BOOL, HSTRING, Vec<u8>, i64), Error>>,
                        )
                    };
                    let (k, v) = if advanced.as_bool() {
                        (
                            HSTRING::from_wide(unsafe { key.as_wide() }).unwrap(),
                            buff.to_vec(),
                        )
                    } else {
                        (HSTRING::default(), Vec::<u8>::new())
                    };
                    ctx_back
                        .send(Ok((advanced, k, v, versionsequencenumber)))
                        .unwrap();
                }
            }
            Err(e) => {
                // no callback will be invoked
                let ctx_back = unsafe {
                    Box::from_raw(
                        ctx_raw as *mut Sender<Result<(BOOL, HSTRING, Vec<u8>, i64), Error>>,
                    )
                };
                ctx_back.send(Err(e)).unwrap();
            }
        }
        rx
    }
}

impl Drop for KeyValueEnumerator {
    fn drop(&mut self) {
        if !self.h.is_null() {
            self.release();
        }
    }
}

impl Default for KeyValueEnumerator {
    fn default() -> Self {
        Self {
            h: std::ptr::null_mut(),
        }
    }
}

#[cfg(test)]
mod test {
    use super::ReliableCollectionRuntime;

    #[test]
    fn test_linking() {
        let _runtime = ReliableCollectionRuntime::create();
    }
}
