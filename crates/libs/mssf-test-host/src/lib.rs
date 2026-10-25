//! Experimental standalone node-side host for one trusted native stateless app.
//! Native-free model tests do not establish native FabricRuntime compatibility.

mod deployment;
mod ipc;
mod process;
#[cfg(test)]
mod tests;
mod wire;

use mssf_transport::{
    message::{ListenInstance, Message},
    serialization::serde::{from_slice, to_vec, EmptyObjectArray, Pointer},
};
use serde_json::json;
use std::{
    fmt,
    net::TcpListener,
    path::PathBuf,
    sync::mpsc::RecvTimeoutError,
    thread,
    time::{Duration, Instant},
};

pub type Result<T> = std::result::Result<T, HostError>;

#[derive(Debug)]
pub struct HostError(pub String);
impl fmt::Display for HostError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for HostError {}
impl From<std::io::Error> for HostError {
    fn from(value: std::io::Error) -> Self {
        Self(value.to_string())
    }
}
impl From<mssf_transport::Error> for HostError {
    fn from(value: mssf_transport::Error) -> Self {
        Self(value.to_string())
    }
}
impl From<std::env::JoinPathsError> for HostError {
    fn from(value: std::env::JoinPathsError) -> Self {
        Self(value.to_string())
    }
}

pub struct Options {
    pub native_directory: PathBuf,
    pub dependency_directory: PathBuf,
    pub executable: PathBuf,
    pub run_parent: PathBuf,
}

#[derive(Default)]
struct Registration {
    started: bool,
    finished: bool,
    runtime: Option<wire::RuntimeContext>,
    registered_type: bool,
    lease_polls: usize,
    handshake: bool,
}

fn require(condition: bool, description: &str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(HostError(description.into()))
    }
}

impl Registration {
    fn dispatch(
        &mut self,
        message: Message,
        session: &mut ipc::Session,
        host: &str,
        pid: u32,
        bundle: &deployment::Deployment,
    ) -> Result<()> {
        let properties = message.properties()?;
        println!(
            "MSSF_HOST {}",
            json!({"event":"incoming", "actor":properties.actor,
            "action":properties.action, "body_bytes":message.body.len()})
        );
        if properties.actor == 1 && properties.action.is_empty() {
            require(!self.handshake, "duplicate transport handshake")?;
            let remote = ListenInstance::decode(&message.body)?;
            require(
                remote.address.is_empty() && remote.instance == 0,
                "expected native client-only listen instance",
            )?;
            self.handshake = true;
            return Ok(());
        }
        require(
            self.handshake,
            "application message before transport handshake",
        )?;
        require(
            properties.relates_to == Default::default(),
            "unexpected correlated app reply",
        )?;
        let identities: Vec<_> = message
            .headers
            .iter()
            .filter(|h| h.id == wire::IPC_HEADER)
            .collect();
        require(
            identities.len() == 1,
            "IPC message must have one identity header",
        )?;
        let identity: wire::IpcIdentity = from_slice(&identities[0].value)?;
        require(
            identity.host == host && identity.pid == pid,
            "unowned IPC host/PID",
        )?;
        require(
            properties.expects_reply,
            "unsupported one-way application traffic",
        )?;
        let mut reply_action = None;
        let body = match (properties.actor, properties.action.as_str()) {
            (7, "StartRegisterApplicationHostRequest") => {
                let request: wire::HostStart = from_slice(&message.body)?;
                require(
                    !self.started
                        && request.host == host
                        && request.pid == pid
                        && request.kind == 1
                        && !request.container
                        && !request.activator
                        && request.timeout > 0,
                    "invalid host start context/state",
                )?;
                self.started = true;
                to_vec(&wire::StartReply {
                    node_id: "1".into(),
                    node_pid: std::process::id(),
                    lease_handle: 1,
                    lease_duration: wire::TTL_TICKS,
                    error: 0,
                    node_instance: 1,
                    node_name: "NativeTestNode".into(),
                    node_type: "NativeTestNodeType".into(),
                    // A closed loopback address, never a real cluster gateway.
                    client_address: "127.0.0.1:0".into(),
                    deployment: bundle.deployment.to_string_lossy().into_owned(),
                    ip: "127.0.0.1".into(),
                    work: bundle.work.to_string_lossy().into_owned(),
                    initial_ttl: wire::TTL_TICKS,
                    application_directories: EmptyObjectArray,
                    node_directories: EmptyObjectArray,
                    app_log: Pointer(None),
                    system_log: Pointer(None),
                })?
            }
            (7, "FinishRegisterApplicationHostRequest") => {
                let request: wire::HostId = from_slice(&message.body)?;
                require(
                    self.started && !self.finished && request.host == host,
                    "invalid host finish state",
                )?;
                self.finished = true;
                to_vec(&wire::ErrorReply { error: 0 })?
            }
            (9, "RegisterFabricRuntimeRequest") => {
                let request: wire::RuntimeRegistration = from_slice(&message.body)?;
                let context = &request.context;
                require(
                    self.finished
                        && self.runtime.is_none()
                        && !context.id.is_empty()
                        && context.host
                            == wire::HostContext {
                                host: host.into(),
                                kind: 1,
                                pid,
                                container: false,
                                activator: false,
                            }
                        && context.code == wire::expected_code(),
                    "invalid runtime registration context/state",
                )?;
                println!(
                    "MSSF_HOST {}",
                    json!({"event":"runtime_registered", "runtime":context.id})
                );
                self.runtime = Some(request.context);
                to_vec(&wire::ErrorReply { error: 0 })?
            }
            (9, "RegisterServiceTypeRequest") => {
                let request: wire::TypeRegistration = from_slice(&message.body)?;
                require(
                    self.runtime.as_ref() == Some(&request.context)
                        && !self.registered_type
                        && request.service_type.package == request.context.code.code.package
                        && request.service_type.name == wire::SERVICE_TYPE,
                    "invalid service type context/state",
                )?;
                self.registered_type = true;
                reply_action = Some("RegisterServiceTypeReply");
                to_vec(&wire::ErrorReply { error: 0 })?
            }
            (15, "") => {
                require(
                    self.started && message.body.is_empty(),
                    "invalid lease request/state",
                )?;
                self.lease_polls += 1;
                to_vec(&wire::LeaseReply {
                    ttl: wire::TTL_TICKS,
                })?
            }
            (9, "UnregisterFabricRuntimeRequest") => {
                let request: wire::RuntimeId = from_slice(&message.body)?;
                require(
                    self.runtime
                        .as_ref()
                        .is_some_and(|runtime| runtime.id == request.id),
                    "unknown runtime unregister",
                )?;
                self.runtime = None;
                self.registered_type = false;
                to_vec(&wire::ErrorReply { error: 0 })?
            }
            (7, "UnregisterApplicationHostRequest") => {
                let request: wire::HostUnregister = from_slice(&message.body)?;
                require(
                    self.finished && request.host == host,
                    "unknown host unregister",
                )?;
                self.finished = false;
                self.runtime = None;
                self.registered_type = false;
                to_vec(&wire::ErrorReply { error: 0 })?
            }
            _ => {
                return Err(HostError(format!(
                    "unsupported native actor/action {}/{}",
                    properties.actor, properties.action
                )))
            }
        };
        session.reply(properties.id, reply_action, body)?;
        Ok(())
    }
}

/// Run only the native registration feasibility gate, not a simulated peer.
pub fn bootstrap(options: Options) -> Result<()> {
    deployment::preflight(&options.native_directory, &options.executable)?;
    let bundle = deployment::Deployment::create(&options.run_parent, &options.executable)?;
    let listener = TcpListener::bind("127.0.0.1:0")?;
    listener.set_nonblocking(true)?;
    let address = listener.local_addr()?.to_string();
    let host = uuid::Uuid::new_v4().to_string();
    let mut child = process::OwnedApp::launch(
        &bundle,
        &options.native_directory,
        &options.dependency_directory,
        &host,
        &address,
    )?;
    println!(
        "MSSF_HOST {}",
        json!({"event":"spawned", "host":host, "pid":child.pid(),
        "profile":"linux-x86_64-11.8.121.4-loopback-clear", "root":bundle.root, "ipc":address})
    );
    let deadline = Instant::now() + Duration::from_secs(120);
    let outcome = (|| -> Result<()> {
        let stream = loop {
            child.check_alive()?;
            require(
                Instant::now() < deadline,
                "native connection deadline exceeded",
            )?;
            match listener.accept() {
                Ok((stream, peer)) => {
                    require(peer.ip().is_loopback(), "non-loopback peer")?;
                    break stream;
                }

                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(10))
                }
                Err(error) => return Err(error.into()),
            }
        };
        let mut session = ipc::Session::new(stream, address)?;
        let mut state = Registration::default();
        loop {
            child.check_alive()?;
            require(
                Instant::now() < deadline,
                "native bootstrap deadline exceeded",
            )?;
            match session.incoming.recv_timeout(Duration::from_millis(50)) {
                Ok(message) => {
                    state.dispatch(message?, &mut session, &host, child.pid(), &bundle)?
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(HostError("native IPC reader stopped".into()))
                }
            }
            if state.registered_type && state.lease_polls > 0 {
                println!(
                    "MSSF_HOST {}",
                    json!({"event":"bootstrap_passed", "lease_polls":state.lease_polls})
                );
                break;
            }
        }
        child.command(b"shutdown\n")?;
        while state.runtime.is_some() {
            child.check_alive()?;
            require(
                Instant::now() < deadline,
                "runtime unregister deadline exceeded",
            )?;
            match session.incoming.recv_timeout(Duration::from_millis(50)) {
                Ok(message) => {
                    state.dispatch(message?, &mut session, &host, child.pid(), &bundle)?
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(HostError("IPC stopped before runtime unregister".into()))
                }
            }
        }
        child.command(b"exit\n")?;
        child.wait_exit(Instant::now() + Duration::from_secs(5))?;
        Ok(())
    })();
    let cleanup = child.cleanup();
    for line in child.logs() {
        println!("MSSF_NATIVE {line}");
    }
    println!(
        "MSSF_HOST {}",
        json!({"event":"cleanup", "result":cleanup.as_ref().map_err(ToString::to_string)})
    );
    cleanup?;
    outcome
}
