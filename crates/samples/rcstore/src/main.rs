// ------------------------------------------------------------
// Copyright 2024 Youyuan Wu
// Licensed under the MIT License (MIT). See License in the repo root for
// license information.
// ------------------------------------------------------------

// This is needed because windows_core macro looks for the `windows_core` token.
extern crate mssf_pal as windows_core;

use mssf_core::WString;
use mssf_core::{debug::wait_for_debugger, runtime::CodePackageActivationContext};
use mssf_util::tokio::TokioExecutor;
use sfrc_core::wrap::ReliableCollectionRuntime;
use tracing::info;

use crate::rcstore::Factory;

#[allow(non_camel_case_types, non_snake_case)]
mod rcstore;
#[allow(non_camel_case_types, non_snake_case)]
mod utils;

fn has_debug_arg() -> bool {
    let args: Vec<String> = std::env::args().collect();
    for arg in args {
        if arg == "-WaitForDebugger" {
            return true;
        }
    }
    false
}

fn main() -> mssf_core::Result<()> {
    tracing_subscriber::fmt().init();
    info!("main start");
    if has_debug_arg() {
        wait_for_debugger();
    }

    // init
    let _init = ReliableCollectionRuntime::create();

    let rt = tokio::runtime::Runtime::new().unwrap();

    let e = TokioExecutor::new(rt.handle().clone());
    let runtime = mssf_core::runtime::Runtime::create(e.clone()).unwrap();
    let actctx = CodePackageActivationContext::create().unwrap();
    let rplctr_endpoint = actctx
        .get_endpoint_resource(&WString::from("ReplicatorEndpoint"))
        .unwrap();

    let grpc_endpoint = actctx
        .get_endpoint_resource(&WString::from("GrpcEndpoint"))
        .unwrap();

    let factory = Factory::create(rplctr_endpoint.port, grpc_endpoint.port, e.clone());
    runtime
        .register_stateful_service_factory(&WString::from("RcStoreService"), Box::new(factory))
        .unwrap();

    e.block_until_ctrlc();
    Ok(())
}
