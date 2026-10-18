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
    runtime::{node_context::NodeContext, CodePackageActivationContext},
    WString,
};
use mssf_util::tokio::TokioExecutor;
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
    rt: TokioExecutor,
    node_address: String,
    replication_port: u32,
    rpc_port: u32,
    workdir: PathBuf,
}

fn endpoint_address(host: &str, port: u32) -> String {
    match host.parse::<std::net::Ipv6Addr>() {
        Ok(address) => format!("[{address}]:{port}"),
        Err(_) => format!("{host}:{port}"),
    }
}

fn main() -> mssf_core::Result<()> {
    tracing_subscriber::fmt().init();
    info!("main start");
    if has_debug_arg() {
        wait_for_debugger();
    }

    let rt = tokio::runtime::Runtime::new().unwrap();
    let e = TokioExecutor::new(rt.handle().clone());
    let runtime = mssf_core::runtime::Runtime::create(e.clone()).unwrap();
    let actctx = CodePackageActivationContext::create().unwrap();
    let endpoint = actctx
        .get_endpoint_resource(&WString::from("KvReplicatorEndpoint"))
        .unwrap();
    let rpc_endpoint = actctx
        .get_endpoint_resource(&WString::from("KvRpcEndpoint"))
        .unwrap();
    let work_dir = actctx.get_code_package_info().work_directory;
    let node_context = NodeContext::get_sync()?;
    let ctx = ProcCtx {
        rt: e.clone(),
        node_address: node_context.ip_address_or_fqdn.to_string(),
        replication_port: endpoint.port,
        rpc_port: rpc_endpoint.port,
        workdir: PathBuf::from(work_dir.to_string()),
    };

    let factory = Factory::create(ctx);
    runtime
        .register_stateful_service_factory(&WString::from("KvMapService"), Box::new(factory))
        .unwrap();

    e.block_until_ctrlc();
    Ok(())
}

#[cfg(test)]
mod endpoint_tests {
    use super::endpoint_address;

    #[test]
    fn ipv4_uses_the_node_address_and_allocated_port() {
        assert_eq!(
            endpoint_address("172.30.81.14", 22017),
            "172.30.81.14:22017"
        );
    }

    #[test]
    fn ipv6_addresses_are_bracketed() {
        assert_eq!(
            endpoint_address("2001:db8::14", 22018),
            "[2001:db8::14]:22018"
        );
    }

    #[test]
    fn hostnames_and_already_bracketed_addresses_are_preserved() {
        assert_eq!(
            endpoint_address("sf-node.example", 22019),
            "sf-node.example:22019"
        );
        assert_eq!(endpoint_address("[::1]", 22020), "[::1]:22020");
    }

    #[test]
    fn onebox_loopback_addresses_remain_valid() {
        assert_eq!(endpoint_address("127.0.0.1", 22021), "127.0.0.1:22021");
        assert_eq!(endpoint_address("::1", 22022), "[::1]:22022");
    }
}
