pub mod app;
pub mod data;
pub mod kvmap;
pub mod rpc;
#[cfg(test)]
mod test;

use std::path::PathBuf;

use kvmap::Factory;
use mssf_core::{
    debug::wait_for_debugger,
    runtime::{
        executor::{DefaultExecutor, Executor},
        CodePackageActivationContext,
    },
    strings::HSTRINGWrap,
    HSTRING,
};
use tracing::info;

fn has_debug_arg() -> bool {
    let args: Vec<String> = std::env::args().collect();
    for arg in args {
        if arg == "-WaitForDebugger" {
            return true;
        }
    }
    false
}

// ctx info for the app.
#[derive(Clone)]
pub struct ProcCtx {
    rt: DefaultExecutor,
    replication_port: u32,
    rpc_port: u32,
    workdir: PathBuf,
}

fn main() -> mssf_core::Result<()> {
    tracing_subscriber::fmt().init();
    info!("main start");
    if has_debug_arg() {
        wait_for_debugger();
    }

    let rt = tokio::runtime::Runtime::new().unwrap();
    let e = DefaultExecutor::new(rt.handle().clone());
    let runtime = mssf_core::runtime::Runtime::create(e.clone()).unwrap();
    let actctx = CodePackageActivationContext::create().unwrap();
    let endpoint = actctx
        .get_endpoint_resource(&HSTRING::from("KvReplicatorEndpoint"))
        .unwrap();
    let rpc_endpoint = actctx
        .get_endpoint_resource(&HSTRING::from("KvRpcEndpoint"))
        .unwrap();
    let work_dir: HSTRINGWrap = unsafe { actctx.get_com().get_WorkDirectory() }.into();
    let ctx = ProcCtx {
        rt: e.clone(),
        replication_port: endpoint.port,
        rpc_port: rpc_endpoint.port,
        workdir: PathBuf::from(HSTRING::from(work_dir).to_string()),
    };

    let factory = Factory::create(ctx);
    runtime
        .register_stateful_service_factory(&HSTRING::from("KvMapService"), factory)
        .unwrap();

    e.run_until_ctrl_c();
    Ok(())
}
