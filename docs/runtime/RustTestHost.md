# Rust standalone SF test host design

## 1. Goal and meaning of standalone

Build a Rust test host that **launches a real Service Fabric application
executable without starting Fabric/FabricHost, FM, Naming, a onebox or a cluster**.
The host supplies the app-facing node Hosting/registration/lease services and
a bounded RA controller. The application retains its real FabricRuntime and,
when selected, its native replicator/storage stack.

This is the opposite boundary from the
[pure Rust application runtime](RustFabricRuntime.md):

```text
Pure Rust runtime:     native node <-> Rust app-side runtime <-> service
Rust test host:        Rust node-side subset <-> native runtime <-> real service
```

**Status:** design only. No clusterless app launch or Rust test-host
interoperability was demonstrated by this documentation work. Native source
baseline is `3988b4518236d2d0a37f4d8bdcdf0a2093c5a61d`; repository baseline is
`e87b47ad1f36e8fa37f3b392bed7a8b2226bee75`, inspected 2026-10-25.
Private protocols must be qualified against the installed app-side runtime,
not assumed to match the repository's 11.8.121.4 image.

**Standalone does not mean SDK/library-free applications, production RA
equivalence, a full application deployer, or a security sandbox.** The Rust host
should have no native SF dependency. A native-runtime app still needs its own
runtime libraries/configuration; a managed app additionally needs its managed
runtime and supported SDK stack. It may also need native replication/logging
components. Absence of a cluster does not remove those dependencies.

## 2. First supported profile

Start with one version-pinned **Linux activated single-code-package ExeHost**,
ordinary stateless factories, and a qualified local IPC transport policy.
The host launches the executable directly with a private environment and
deployment directory. It implements the node-facing contracts, not a fake
`FabricCreateRuntime` DLL or an in-process COM factory wrapper.

| Profile | Intended coverage |
|---|---|
| Initial stateless | Real runtime boot, type registration, open/close/abort, real application RPC and reporting |
| Later singleton stateful | Native default replicator under a deliberately single-replica test configuration; no HA claim |
| Later multi-process stateful | Several native app processes with real peer replication and scripted RA reconfiguration |
| Separate extensions | Container/TLS, Windows security, nonactivated/DLL/multicode-package hosts, managed SDK variants, upgrades and service groups |

This is not a promise to run every SF app unchanged. Apps that require Naming,
FabricClient management, secrets/certificates from deployment policy, activator
services, DNS, RunAs/container isolation or external dependencies need explicit
adapters/profiles or must be rejected as unsupported.
Guest executables that never create/register a FabricRuntime do not acquire a
RAP simply because the host injects environment variables.

## 3. Why a small RA-only listener is insufficient

The native runtime has a startup dependency graph:

```text
native loader/configuration
  -> activated host identity and runtime connection
  -> host start/finish registration and lease/context information
  -> native runtime subsystem opens (replicator factory, RAP, KTL/log manager...)
  -> deployment-file-backed activation context
  -> runtime registration
  -> service-type registration
  -> RA ReplicaOpen
  -> actual factory/Open callback and usable application endpoint
```

Even stateless native bootstrap initializes KTL/logging-related components;
this is different from a future stateless Rust runtime choosing not to ship
those engines. The inspected Unix log-manager default is in-process, but that
does not prove every native bootstrap/storage path needs no additional service
or OS facility.
[Native startup][native-open]; [log-manager mode/open][log-manager].

The first implementation milestone must therefore prove **native bootstrap**.
Do not start by implementing a large command API over a server that returns
success-shaped placeholder registration data.
See [node-side architecture](FabricRuntimeHost.md) for the complete responsibility split.

## 4. Proposed implementation shape

Use one `mssf-test-host` crate with a Rust library and a thin CLI initially:

```text
test scenario / optional CLI
      |
TestHost: scenario state, owned resources, deadlines and event journal
      +-- process supervisor: child handles, environment, exit/log collection
      +-- deployment materializer: selected package/profile -> private run layout
      +-- registration server: host/runtime/type state and reverse connection
      +-- lease service: finite test TTL and explicit expiry/invalidation
      +-- RA controller: supported service-unit transitions and semantic replies
      +-- local report sink: load/fault/health outcomes
      +-- application probe: real RPC, separate from protocol decoding
      |
generic Fabric IPC server over mssf-transport primitives
      |
real app process: native FabricRuntime + service implementation
```

Keep protocol and scenario state in the library; a CLI must not be the only
way tests observe an ordered event or own the child lifetime. Reuse serialization,
frames, message/header primitives and bounded I/O from `mssf-transport`.
Its current connection driver is client-only and its reverse path is
NamingGateway-notification-specific, so it is **not already the required server**.
[Existing transport limitations](RustFabricRuntime.md#3-what-this-repository-can-actually-reuse).

Implement the general server/IPC session without changing existing gateway
admission rules. Keep native wire DTOs separate from scenario models. If the
application-side Rust runtime is implemented too, share genuinely common
wire definitions; do not make the test host depend on a native COM adapter or
the application runtime's supervisor merely to avoid duplicating a struct.

### Conceptual library API

```text
TestHost::start(profile, owned_directory) -> TestHost
TestHost::launch(app_spec) -> RunningApp
RunningApp::wait_for_type(type_identity, deadline) -> RegisteredType
RegisteredType::open_stateless(service_spec, deadline) -> ServiceHandle
ServiceHandle::endpoint() -> observed native Open endpoint
ServiceHandle::close(deadline)
ServiceHandle::abort(deadline)
TestHost::events() -> ordered, bounded event subscription
TestHost::shutdown(deadline) -> cleanup report
```

These are proposed concepts, not existing APIs. Stateful controllers would be
added only with an explicit role/configuration model, not a generic
`change_role(Primary)` convenience that skips native reconfiguration.

## 5. Launch inputs and deployment materialization

### 5.1 Explicit launch specification

An `AppSpec` should declare the executable/arguments, package inputs, service
types, endpoint names, native-runtime profile and only the additional environment
the app actually requires. Launch without shell evaluation.
Set the child working directory according to the supported entry-point
WorkingFolder policy; the executable directory and activation work directory
are not automatically interchangeable.

The host creates a fresh host ID, node identity/incarnation and package/runtime
scenario identity. The **runtime ID comes from real runtime registration**;
do not predict it and send commands to a made-up ID.
Host ID, service-package activation identity, package instance sequences,
partition ID, replica/instance IDs and epochs remain distinct.

Activated-host environment includes the paired host ID/type, role booleans,
selected runtime address and complete code-package identity/version/sequence
context. Use the actual child PID observed by the supervisor and compare it
with registration; do not impersonate an existing node/app process.
The synthetic node process ID can identify the actual owning Rust host.
[Bootstrap environment](FabricRuntime.md#41-host-variants-and-environment).

Supply a declared native library/managed-runtime location and a private native
configuration file. Native `FabricConfigFileName`, `FabricPackageFileName` and
`FabricSettingsFileName` selection can otherwise discover installed/module-local
configuration. Clear conflicting inherited SF settings and use the selected
profile's explicit config-store input. Ordinary loader/PATH requirements remain
platform-specific; do not rewrite machine-wide configuration or install a
service as part of a test.
[Config-store selection][native-config].

### 5.2 A deployment bundle, not just two manifests

Support a **validated test deployment bundle** first; general application-package
deployment is later scope. Native activation reads digested application/service
package descriptions, resolved endpoints and principals in addition to the
developer's ApplicationManifest/ServiceManifest.
[Native artifact reader][activation-reader]; [principals read][principals-reader].

For a SharedProcess fixture, the layout has the following source-derived shape:

```text
owned-root/
  deployment/
    <native-application-id>/
      App.<application-rollout>.xml
      <service-package>.Package.Current.xml
      <manifest-name>.Manifest.<manifest-version>.xml
      <service-package>.Endpoints.txt
      Application.Sids.txt
      <service-package>.<code-package>.<version>/
        <executable and declared dependencies>
      <service-package>.<config-package>.<version>/
        Settings.xml
      work/
      log/
      temp/
  node-work/
  native-config/
  diagnostics/
```

Config/data package folders exist only when declared. ExclusiveProcess scopes
the current-package/endpoints names by public activation ID; shared packages
have additional layout rules. Do not apply this simplified SharedProcess tree
to every host/profile.
[Run-layout methods][run-layout]; [filename/folder patterns][name-patterns].

`Endpoints.txt` is not JSON/XML in this path. Native writer/reader uses
comma-separated endpoint records with **nine semicolon-separated fields**:
name, protocol, type, port, certificate reference, URI scheme, path suffix,
code-package reference, IP/FQDN. Preserve empty fields, validate delimiters and
qualify encoding with the native reader. `Application.Sids.txt` is also a
separate native-read file; even a no-principals profile must satisfy that read.
[Endpoint file format][endpoint-file]; [principal file format][principal-file].

A bundle must specify matching rollout/version/identity relationships and
resolved endpoint allocations. Prefer provenance-bearing native-generated
nonsecret test artifacts or a source-derived generator that is qualified by
actual native parsing. Never copy live production credentials, SIDs or state
into a fixture. Rewriting paths/identities invalidates "native generated" as a
complete correctness argument; the resulting bundle must still boot the native
runtime and expose the expected context.

### 5.3 Ports and process readiness

Bind the host IPC listener on an owned loopback ephemeral port and keep it open.
Application endpoint ports need a separate allocation policy: closing a probe
socket does not reserve a port for a later child bind. Use controlled ownership/
allocation plus bounded collision detection and rebuild/relaunch the owned
fixture when necessary. Never take over a port used by another test/cluster.

Do not use sleeps as readiness. Observe host/runtime/type registration, send
the legal open command, await the **semantic open reply**, and independently
probe the returned application endpoint. Flushing the type-registration reply
does not prove the app's asynchronous local factory validation already ran;
native-compatible same-incarnation retry must tolerate that race.

## 6. Node IPC services the host must supply

### 6.1 Server transport and routing

Implement server-role native connection establishment, including ListenInstance,
normal controls and the selected security profile. Receiving clear frames is
not a complete server handshake.

Maintain `connection -> owned child/host incarnation` and `HostId -> connection`
bindings, learned/validated during registration. Decode and check `IpcHeader`
where native client sends require it; attach `RelatesTo` to actual correlated
replies. Native node-initiated one-way commands do not acquire an
`ExpectsReply`/pending-request entry just because the scenario awaits completion.
See [native IPC routing](FabricRuntime.md#5-ipc-and-message-routing).

Use bounded queues, decoder/allocation limits and outstanding-request caps;
reserve reply/lease/close capacity independently of ordinary reports.
The current Rust frame/object bounds and native IPC limits differ. Establish
qualified per-profile bounds instead of silently accepting truncated bodies
or globally removing the existing limits.

### 6.2 Required actor/action surface

| Incoming app traffic | Host implementation |
|---|---|
| Actor 7 / host start | Validate owned expected host/package mode/PID; create pending registration; return a complete start reply |
| Actor 7 / host finish | Validate pending/completed state; complete registration and reply idempotently as qualified |
| Actor 7 / host unregister | Invalidate owned host/runtime/type/unit state, reply, retire route safely |
| Actor 9 / runtime register/unregister | Enforce host state and context; learn actual runtime ID; retain tombstones for obsolete incarnations |
| Actor 9 / service-type register | Validate runtime/package/activation identity and declared type; preserve same-runtime versus conflicting registration outcomes |
| Actor 15 / empty lease poll | Return finite TTL for this synthetic node/session; no invented action name |
| Actor 12 / lifecycle semantic reply | Decode reply body and match pending semantic operation, not transport correlation |
| Actor 12 / load/fault/endpoint/access traffic | Record and apply scenario policy; send the defined acknowledgements where required |
| Actor 12 or 7 / health report | Decode reports/sequence streams; retain locally; return conforming per-report/stream results |
| IPC reconnect/control | Apply selected policy to the same owned incarnation; never adopt an unrelated client |

Exact schemas and reply patterns:
[Hosting messages](FabricRuntime.md#43-hosting-messages-and-validation) and
[RA control](FabricRuntimeHost.md#4-the-host-to-runtime-protocol).
Not all hosting replies have an action header. In particular, do not require or
invent `StartRegisterApplicationHostReply`.
Service-type registration also does not transmit a factory pointer or a
stateful/stateless discriminator. Take service kind/settings from the declared
bundle/scenario and validate their consistency, rather than infer them from a
request that does not carry them.

Unknown mandatory actions, unsupported security, malformed messages or a
registration/context mismatch must produce a structured test failure. Use a
native error reply only where that protocol defines one; otherwise terminate
the owned session/scenario. Never respond with generic success to keep bootstrap
moving. Unsupported optional traffic also needs a documented disposition and
diagnostic, not an unnoticed drop.

### 6.3 Start reply and native subsystem inputs

The start reply carries 17 fields, including canonical native node ID, the
Rust test-host PID in `nodeHostProcessId` (not the child PID), numeric lease
handle/duration/initial TTL, node metadata, client address, deployment/work/logical
directories and **two shared-log settings pointers**. All must be structurally
correct even when unused by the particular service.
[Start reply schema][start-reply].

Synthetic node identity is test authority, not a registration in a real cluster.
Never return a real cluster gateway address or borrow a real node lease handle.
The default Linux IPC-monitor profile stores an opaque non-null lease token;
qualify that branch explicitly, including any native validation. A mode that
switches to direct native lease queries is unsupported, not an invitation to
forge an OS handle.

Shared-log settings are not an arbitrary path string: the native object has
seven fields, with a fixed 512-WCHAR path serialized as a byte array, IDs,
size/stream/record limits and flags. Optional/null settings are acceptable only
where the selected native path supports them and real bootstrap confirms it.
For stateful profiles, provision isolated valid log paths/settings rather than
all-zero stand-ins.
[Shared-log schema][shared-log]; [optional log-manager inputs][log-manager].

Return an explicit unsupported-capability failure if an app requests a local
FabricClient/gateway operation the profile does not implement. Leaving the
client address empty and allowing fallback to a real cluster is not isolation.

### 6.4 Finite lease and report behavior

The host simulates **test-local node authority**, not federation leasing.
Use bounded real-monotonic TTLs and an explicit healthy/expired/invalidated state.
Keep initial TTL conversion as a deployed-version qualification gate: the
inspected initial path differs from later request-send-time-plus-TTL polling.
Do not compensate with an infinite lease or a fabricated absolute TTL.
[Native lease semantics](FabricRuntime.md#92-default-linux-app-host-lease-monitoring-uses-ipc).

A cautious initial scenario can wait for an observed valid poll before sending
service open; stateful authority must also be checked through actual application
behavior/access status. Expiry tests must wait/measure native monotonic behavior:
advancing only a Rust fake clock does not advance a native process's lease timer.
`InvalidateLeaseRequest` closes the app lease monitor; it is not equivalent to
process death or guaranteed termination of every stateless listener.

Load/report storage should be observable as local test events. Health acceptance
means **accepted by the local test sink**, not accepted/aggregated by a real HM.
Decode and acknowledge each supported report/sequence stream using its native
identity/sequence semantics; a bare `ErrorCode` or empty successful body is not
the health reply contract.
[Health reply decoder][health-decoder]; [health reply schema][health-reply].
If broader health-progress APIs appear, support them as an explicit profile
extension or fail the capability check rather than creating endless retries.

Fault policy should be explicit: ordinary tests can assert the report and then
choose source-shaped restart/drop/abort behavior; recovery tests enable that
transition. Logging a fault without applying the selected policy must not be
described as native RA recovery.

## 7. The bounded RA controller

### 7.1 Service-unit records and pending operations

Keep process/connection ownership separate from runtime/type registration and
from each service unit. A unit records the host generation, runtime ID,
partition/consistency information, replica ID/instance, native node identity,
service/package description, epochs/configuration, last observed endpoints
and pending operation.

Semantic matching uses connection/host incarnation, partition, replica/instance,
expected reply action and operation-specific epoch/flag rules. Most reply bodies
do not echo runtime ID; infer it from the owned active unit, not an invented
wire field. One-way retries reuse the same intended identity/configuration.
Do not issue indistinguishable conflicting operations and hope MessageId
separates their replies.

Retire old identities on process exit/restart and retain bounded tombstones so
late replies cannot complete a new run. Timeouts mean **remote outcome may be
unknown**: compensate with confirmed close/abort or terminate the child; do not
report "never opened" because the Rust waiter timed out.
[Native reply fencing](FabricRuntimeHost.md#52-matching-completion-requires-semantic-identity).

### 7.2 Initial stateless state machine

```text
ProcessStarting
  -> HostRegistered -> RuntimeRegistered -> TypeRegistered
  -> OpenPending -> Open
  -> ClosePending -> Closed
  -> process shutdown/reap

Any stage -> failure/timeout -> bounded cleanup -> reaped or cleanup failure
```

For open, build a valid native stateless service description and
`ReplicaOpen` body for the **registered** runtime/type. Use native stateless
role semantics, not a pretend stateful Primary. Await matching
`ReplicaOpenReply`, retain native error/text, then probe its endpoint.
Retry lost replies without creating a new instance.

Close sends `ReplicaClose` for the same unit; abort uses the native abort flag
and legal state. A close reply is not process exit. A failed user close may
still yield successful terminal close after native abort cleanup; observe
callback diagnostics separately where the test application exposes them.
[Native lifecycle behavior](FabricRuntime.md#62-stateless-path).

One process can host several **different partitions**. Do not attempt several
simultaneous replicas of the **same partition** inside one native app-host RAP:
its local proxy map is keyed by failover-unit ID. Multi-replica scenarios need
separate app-host processes.
[Proxy map key][proxy-map].

### 7.3 Stateful extensions

The first stateful experiment can use a deliberately declared singleton test
configuration with `MinReplicaSetSize = TargetReplicaSetSize = 1`, matching
manifest/bundle and wire service description. This is an explicit test variant,
not a silent rewrite of production quorum policy and not an HA/quorum test.
The repository's `kvmap` currently calls the native replicator; it cannot serve
as proof of a Rust replication engine.
[Existing sample][kvmap].

Multi-process stateful tests give each synthetic node/app process a distinct
node/host incarnation and actual native replication endpoint. Drive the
source-defined open/build/configuration/catch-up/end-reconfiguration sequence;
await its phase-appropriate replies and acknowledge endpoint/access-revocation
notifications. Honor previous/current configurations and native epoch field
encoding; a role name alone does not grant write permission.
[RA configuration control](FabricRuntimeHost.md#6-stateful-control-is-a-multi-stage-protocol).

The native replicas handle their direct copy/replication traffic. The Rust test
host controls placement/roles **only for the test**; it does not implement a
production election algorithm or a second consensus layer. Explicit failover
scripts must fence/demote/kill the old primary appropriately, not promote two
independent primaries and call observed divergence a native protocol defect.
The host does not instantiate native RA, so it need not emulate FM node-up
handshakes merely to reach RA's internal readiness gate. It must still implement
the actual app-facing control sequence; bypassing the node state machine does
not make every arbitrary sequence legal.

Persisted restart uses the same owned data intentionally with a new appropriate
host/replica incarnation and native reopen flow. Ordinary tests start fresh.
Never load arbitrary preexisting native store data or claim disk compatibility
from an in-memory successful write.

## 8. Scenario control, observation and isolation

Keep a typed event journal: process spawned/exited, connection established,
registration transitions, command sent/retried, reply accepted/stale,
endpoint observed, report retained/acknowledged, lease state and cleanup.
Include monotonic timestamps, owned identities and native errors; omit secrets
and bound output/log retention. Record the selected source/runtime/profile
versions with every result.

Expose barriers over observed events, not polling sleeps or unrestricted raw
packet injection. Fault injection may withhold/reorder/drop selected test
commands/replies or stop a managed process. A delayed send must retain its
original identity and become inert after cancellation/teardown. Unknown or
expired events must not silently become successful steps.

A typed scenario API should make unsafe default sequences hard to express.
Exceptional stale/duplicate-message tests use explicit negative-test operations;
they must not mutate normal identity tracking to satisfy an expected result.

Own each child, its process group/job and its descendant lifetime. Run only
explicitly selected trusted test programs: this harness is not an execution
sandbox. Use an isolated test network/container profile on shared machines,
dedicated directories, unprivileged identities and no real cluster credentials.
Loopback plus claimed PID/HostId is not authentication against other local
processes. Do not expose the IPC/control server on a public interface.
Unsupported secure deployments must fail; never disable native security or
runtime-access-removal policy to make an app attach.

### Cleanup contract

Stop new scenario work, request legal service close/abort where possible, collect
native unregister traffic, close sessions, terminate only still-owned child
handles after a bounded grace period, and **wait/reap** them. Preserve whether
cleanup was graceful, abort-driven or forced. A CLI killed by its parent also
needs an owned-child lifetime strategy; async destructors alone are insufficient.
If an owned child/descendant cannot be terminated and reaped by the deadline,
return an explicit cleanup failure with its managed identity, not a clean-run
result or a process-name-based kill fallback.

Resolve cleanup paths under the run's canonical owned root, reject escaping
symlinks/paths, and delete only known owned artifacts after handles are closed.
Preserve bounded diagnostics on failure. Do not delete a shared root, kill
processes by name, stop a running onebox, or modify machine SF services.

## 9. Milestones and acceptance evidence

| Milestone | Deliverable | Exit evidence |
|---|---|---|
| 0: native profile | Pin app runtime/build and source mapping; define a minimal executable plus complete test deployment bundle | App dependencies/config/file requirements known; no hidden connection to an existing cluster |
| 1: server bootstrap | Server-role transport, registration, finite lease, required report routes and native startup inputs | A real native-runtime child creates runtime, reads context, registers a type and stays healthy; no Fabric/FabricHost/node process started |
| 2: stateless control | Legal open/close/abort, semantic reply tracking and app probes | Real factory/lifecycle callbacks and usable RPC; returned IDs/context/endpoints independently checked; duplicate/lost reply does not duplicate instances |
| 3: resilience/isolation | Failure paths, deadlines, late replies, concurrent partitions and verified owned cleanup | No cross-run completions/port collisions/resource leaks; native errors remain visible; stuck child is reaped or cleanup fails explicitly, with forced cleanup distinguished |
| 4: singleton stateful | Selected native replicator/storage profile, explicit one-replica fixture and legal role flow | Actual state reads/writes and persisted reopen where promised; access/lease fencing observed, not inferred from role alone |
| 5: multi-process stateful | Source-shaped membership/reconfiguration scripts and native peer endpoints | Real copy/catch-up, old-primary fencing, failover and independent state oracle; no production-RA-equivalence claim |
| 6: extra profiles | One selected SDK/platform/security/upgrade feature at a time | Separate positive/negative compatibility matrix for each advertised profile |

Milestone 1 is the go/no-go feasibility gate. If native bootstrap still requires
an untraced native service/configuration dependency, report that requirement or
limit the profile; do not hide it behind a stubbed native API.

## 10. What these tests establish

| Setup | Useful evidence | Not established |
|---|---|---|
| Rust test host + real native-runtime app | App lifecycle/context behavior, real native wire acceptance and controlled failure cases for the tested profile | Production FM/placement/node-recovery correctness |
| Several native app processes + scripted host | Real native replication/copy and app behavior under the scripted membership transitions | Full production RA reconfiguration coverage or physical-node failure-domain behavior |
| Rust test host + future pure Rust runtime | Deterministic end-to-end model tests and debugging | Independent protocol compatibility if both sides share the same mistaken schema |
| Native node + future Rust runtime | Actual node interoperation for the measured scenarios | Every release/host/security/storage variant |

For positive qualification, execute a real service workload and observe callback/
context values through an independently implemented test app or instrumentation,
and call the service's actual RPC endpoint. A Rust-generated request decoded by
Rust, a canned endpoint string or a log line "factory registered" is not an oracle.

Require negative tests for wrong host/runtime/type context, malformed or stale
messages, callback failure, close during open, dropped/late semantic replies,
lease expiry and child exit. For timeouts assert uncertain outcome/cleanup,
not fictitious rollback. For health assert retained matching report/sequence
results, not that a socket write succeeded. For persistence, kill/restart and
compare independently recorded committed state.

Keep most harness logic native-free unit tested, but clearly separate those
model tests from **native app integration tests**, which require runtime
libraries. Package a version-pinned test environment that starts no SF node
services. Prove that boundary through process/network observations and explicit
dependency/runtime diagnostics, not an environment variable named "standalone."
Native collection/stateful requirements are separate profile prerequisites.

## 11. Remaining feasibility questions

The source supports a credible clusterless host design, but these still require
actual native experiments: server-role handshake and deployed security policy;
complete nested registration/control serializers; initial TTL behavior;
native startup/config/KTL dependencies; valid deployment/principal/endpoint
artifacts; complete health result/sequence behavior; and exact stateful control
scripts. No current file in this repository implements the proposed host.

This deliberately avoids making a production RA rewrite a prerequisite for
app tests. It also avoids selling a registration-only mock as a working SF host.

## Source pointers

The [node-side architecture](FabricRuntimeHost.md) and
[application-side architecture](FabricRuntime.md) contain the broader pinned
source maps. Additional bootstrap/materialization evidence follows.

[native-open]: https://github.com/microsoft/service-fabric/blob/3988b4518236d2d0a37f4d8bdcdf0a2093c5a61d/src/prod/src/Hosting2/ApplicationHost.cpp#L1090-L1275
[log-manager]: https://github.com/microsoft/service-fabric/blob/3988b4518236d2d0a37f4d8bdcdf0a2093c5a61d/src/prod/src/data/logicallog/LogManager.cpp#L250-L332
[native-config]: https://github.com/microsoft/service-fabric/blob/3988b4518236d2d0a37f4d8bdcdf0a2093c5a61d/src/prod/src/Common/FabricEnvironment.cpp#L12-L137
[activation-reader]: https://github.com/microsoft/service-fabric/blob/3988b4518236d2d0a37f4d8bdcdf0a2093c5a61d/src/prod/src/Hosting2/CodePackageActivationContext.cpp#L306-L425
[principals-reader]: https://github.com/microsoft/service-fabric/blob/3988b4518236d2d0a37f4d8bdcdf0a2093c5a61d/src/prod/src/Hosting2/CodePackageActivationContext.cpp#L262-L281
[run-layout]: https://github.com/microsoft/service-fabric/blob/3988b4518236d2d0a37f4d8bdcdf0a2093c5a61d/src/prod/src/Management/ImageModel/RunLayoutSpecification.cpp#L25-L206
[name-patterns]: https://github.com/microsoft/service-fabric/blob/3988b4518236d2d0a37f4d8bdcdf0a2093c5a61d/src/prod/src/Management/ImageModel/NamePatterns.cpp#L16-L143
[endpoint-file]: https://github.com/microsoft/service-fabric/blob/3988b4518236d2d0a37f4d8bdcdf0a2093c5a61d/src/prod/src/ServiceModel/EndpointDescription.cpp#L272-L390
[principal-file]: https://github.com/microsoft/service-fabric/blob/3988b4518236d2d0a37f4d8bdcdf0a2093c5a61d/src/prod/src/ServiceModel/PrincipalsDescription.cpp#L327-L413
[start-reply]: https://github.com/microsoft/service-fabric/blob/3988b4518236d2d0a37f4d8bdcdf0a2093c5a61d/src/prod/src/Hosting2/StartRegisterApplicationHostReply.h#L85-L124
[shared-log]: https://github.com/microsoft/service-fabric/blob/3988b4518236d2d0a37f4d8bdcdf0a2093c5a61d/src/prod/src/ktllogger/SharedLogSettings.h#L33-L130
[health-decoder]: https://github.com/microsoft/service-fabric/blob/3988b4518236d2d0a37f4d8bdcdf0a2093c5a61d/src/prod/src/client/HealthReportingComponent.cpp#L314-L416
[health-reply]: https://github.com/microsoft/service-fabric/blob/3988b4518236d2d0a37f4d8bdcdf0a2093c5a61d/src/prod/src/ServiceModel/management/HealthManager/ReportHealthReplyMessageBody.h#L12-L47
[proxy-map]: https://github.com/microsoft/service-fabric/blob/3988b4518236d2d0a37f4d8bdcdf0a2093c5a61d/src/prod/src/Reliability/Failover/ra/LocalFailoverUnitProxyMap.cpp#L58-L150
[kvmap]: ../../crates/samples/kvmap/src/kvmap.rs#L210-L259
