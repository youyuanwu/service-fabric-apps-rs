# Linux Service Fabric container cluster: design and current state

Updated: 2026-10-18. Status: **experimental, single-host development use**.

This document records the implemented architecture, its validation boundaries,
and deferred work. Build/run commands and recovery instructions live in the
[lifecycle guide](LinuxContainerCluster.md).

## 1. Current state

| Area | Current state |
| --- | --- |
| Reusable infrastructure | Implemented Compose-backed Python CLI: `create`, `up`, `status`, `down`, and explicitly confirmed `destroy`. Fresh clusters have three seed nodes and native baseline `11.8.121.4 / 1`. |
| Persistent lifecycle | Locally exercised creation, health, container/network removal with retained data, recreation/resume, image-tag replacement, and scoped/idempotent destruction. |
| CI | Separate `Linux cluster lifecycle` job added alongside Windows CI for every PR and push to `main`. Hosted execution has not yet been validated. |
| Native configuration upgrades | G2 proved native baseline establishment, a separate configuration-only rollout, and upgraded-state restart/recreation. The CLI automates baseline setup, not arbitrary upgrades. |
| Dynamic join | G3 proved a fresh non-seed join, canonical configuration rollout, and two interruption boundaries. The corrected timed retest passed; the original timed run failed. A reusable `join` command is **not implemented**. |
| Rust application integration | G4's first chunk proved routable endpoints, Linux runtime/COM activation, and cross-node write/read. Failover and crash-safe durability are **not established**. |

**The current deliverable is lifecycle infrastructure and CI, not further
cluster/application fault experiments.** G4 failover/durability work and the
wider fault matrix are deferred, not the automatic next step.

### Retained validation cluster versus CLI-created clusters

The corrected G3/G4 project, `sf-apps-g3-20261017-69438d27-r1`, remains running:
four SF nodes n0-n3 plus its original administrative tools container. Its subnet
is `172.30.81.0/24`, native configuration is `3`, and all mounted bootstrap
inputs remain version `2`. The last operational readiness check observed healthy
nodes/applications after a targeted retained-state n3 restart. That check did
not verify acknowledged application data after recovery.

This project is a retained proof environment, **not an allocation managed by the
new lifecycle CLI**. Its inputs are in the local
`.paw/validation/linux-container-cluster/g3-retest-20261017/join/` directory.
Do not rerun a preparer over its volumes, use an old three-node Compose file to
manage it, or assume the CLI can adopt it.

The separate lifecycle-verification project `sf-linux-infra-69438d27` was
explicitly destroyed after verification. CLI-created clusters use their own
state under ignored `build/linux-clusters/NAME/`, independent of `.paw`.
Historical shutdown observations below describe the end of each experiment,
not the current state of the retained G3/G4 project.

## 2. Decision and scope

Use **one SF node per Docker container**, not one complete scale-min onebox per
container. Docker Compose owns the single-host resource topology; a small Python
controller owns lifecycle coordination and actual SF readiness.

```text
Linux x86-64 host: Python lifecycle controller + local Docker daemon
  |
  +-- project-specific Docker bridge
       |
       +-- tools  .10  administrative only; on demand in the lifecycle CLI
       +-- n0     .11  seed; independent persistent volume
       +-- n1     .12  seed; independent persistent volume
       +-- n2     .13  seed; independent persistent volume
       |
       +-- n3     .14  non-seed in the G3 proof / deferred generic join
```

Addresses are offsets within a selected private subnet, not fixed host-wide
addresses. The CLI creates only n0-n2; n3 is not predeclared or created by Compose
scaling. The original proof environment retains tools continuously, whereas the
CLI starts/removes tools around commands that need cluster access.

| Owner | Responsibility |
| --- | --- |
| Python controller | Allocation records, subnet checks, frozen inputs, ownership checks, serialized operations, native baseline setup, deadlines, and explicit success/failure. |
| Docker/Compose | Containers, bridge, static addresses, namespaces, and named volumes. Container startup order is not a readiness protocol. |
| Node entrypoint | Local identity/bootstrap validation, platform/users/permissions, tracing, current-deployment restart, and FabricHost supervision. |
| Service Fabric | Federation membership, system replicas, service placement, native configuration rollout, and health. |

All containers share a kernel and physical failure domain. Per-container fault
domains describe process/container separation, not independent hosts. Stopping
the host stops the whole cluster.

Out of scope for the current CLI:

- Production support/availability, multi-host networking, and X.509 provisioning.
- In-place conversion of a scale-min onebox, migration/adoption of existing
  clusters, runtime code upgrades, seed replacement, node-type changes, and
  general node removal.
- SF-managed Docker application workloads inside node containers. Current
  application proofs use native Linux processes.
- Generic join/upgrade transactions and automatic destructive rollback.

The topology is an experiment, not an official standalone Linux support claim.
Microsoft's [Linux development documentation][linux-dev] and
[native management APIs][sfctl-cluster] do not establish support for this
deployment. Keep the runtime/schema pinned.

## 3. Implemented surfaces

| Surface | Responsibility |
| --- | --- |
| [`docker/linux-cluster/Dockerfile`](../docker/linux-cluster/Dockerfile) | Self-contained Ubuntu 22.04 amd64 image; pinned base digest, SF package/checksum, Fabric executable checksum, and sfctl version; retained baseline package. No sibling checkout/private base image required. |
| [`node-entrypoint.py`](../docker/linux-cluster/node-entrypoint.py) | Seeded volume identity, fresh-only deployer `Create`, current-state restart, required platform setup, LTTng, and FabricHost signal/shutdown handling. |
| [`ClusterManifest.template.xml`](../docker/linux-cluster/ClusterManifest.template.xml) | Three-seed, non-scale-min topology, generated file-store secrets, explicit system replica settings, native-process hosting, and disabled nested-container setup. |
| [`cluster-probe.py`](../docker/linux-cluster/cluster-probe.py) | All-gateway node/XML/health/native-baseline checks, system replica readiness, and existing application health. |
| [`scripts/linux_cluster_ctl.py`](../scripts/linux_cluster_ctl.py) | Named lifecycle commands, private state/locking, external volumes, image retention, ownership checks, baseline reconciliation, and readiness. |
| [`scripts/linux_cluster_diagnostics.py`](../scripts/linux_cluster_diagnostics.py) | Allowlisted CI metadata/status summaries without raw manifests, environment variables, or arbitrary health/error descriptions. |
| [`scripts/linux_cluster.py`](../scripts/linux_cluster.py) | Shared static generation and the original allocation-only G1 preparer. Its legacy CLI is not the lifecycle or join controller. |
| `scripts/test_linux_cluster*.py`, `scripts/test_linux_node_entrypoint.py` | Generator, lifecycle, identity, image retention, readiness/stability, diagnostics privacy, restart, shutdown, and ownership coverage. Linux-only controller tests skip before importing `fcntl` on Windows. |
| [`docker/linux-cluster/smoke/`](../docker/linux-cluster/smoke/) | Native C guest-executable HTTP package used for remote-activation proof; not deployed implicitly by lifecycle commands. |
| [`kvmap/src/main.rs`](../crates/samples/kvmap/src/main.rs), [`kvmap.rs`](../crates/samples/kvmap/src/kvmap.rs) | NodeContext-based replication/RPC addresses, IPv6-safe formatting, and FQDN bind resolution. |
| [`justfile`](../justfile) | Thin `cluster-build` and `cluster ...` aliases; no separate lifecycle logic or implicit reset. |
| [`.github/workflows/ci.yaml`](../.github/workflows/ci.yaml) | Independent Ubuntu lifecycle job alongside the existing Windows build/application job. |

Compose is generated as JSON, avoiding a YAML templating dependency. The Python
controller uses the standard library and Docker argument arrays. Cluster API
operations run inside tools with an isolated sfctl home, not an unrelated
developer's active connection.

## 4. Topology, networking, and security invariants

The full Linux manifest uses `Linux IsScaleMin="false"` and one common
`LinuxNodeType`. Local-node selection must match exactly one assigned container
address; the [Linux schema][linux-schema], [local-node selection][node-selection],
and [address-matching code][address-matching] explain that contract. Literal
IPv4 is the validated runtime configuration.

- Exactly n0-n2 are seeds; ordinary joins preserve their names, addresses, and
  seed flags. The shared [seed infrastructure][seed-infrastructure] supplies
  federation voting/address information.
- Each node has a unique name, IP, hostname, volume, and upgrade domain. Current
  fault domains are `fd:/host0/c0`-`c2` and upgrade domains `UD0`-`UD2`; the G3
  non-seed uses `fd:/host0/c3` and `UD3`.
- Cluster Manager, Failover Manager, Image Store, and Naming target three
  replicas with minimum two; Naming has one partition. Keep `fabric:ImageStore`.
- Compare parsed active XML, not raw serialization. The runtime omits the
  proven defaults `IsScaleMin=false` and
  [`IsSeedNode=false`][node-seed-default]. Do not normalize away a changed seed,
  node, version, credential, or setting.
- Distinguish mounted bootstrap inputs from active deployment XML. A retained
  node restarts from its current local deployment, which can be newer than the
  immutable bootstrap input.

| Endpoint | Port/range | Current exposure |
| --- | --- | --- |
| Native client | 19000 | Cluster bridge; available for SDK clients. |
| Lease | 19001 | Between nodes. |
| Federation/cluster connection | 19002 | Between nodes and seeds. |
| Service connection | 19006 | Runtime/service traffic. |
| HTTP gateway | 19080 | Cluster bridge; new lifecycle allocations also publish n0 at host `127.0.0.1:19080`. |
| Dynamic application endpoints | 22001-23000 | Directly routable between nodes and clients on the bridge. |
| Native smoke HTTP endpoint | 8080 | Explicit service port outside the dynamic application range. |
| Replicator/file-transfer endpoints | Runtime/manifest-derived | Use actual allocated endpoints; the table is not an exhaustive listener inventory. |

The same ports can be used in separate container network namespaces. Do not
advertise `localhost`, wildcard addresses, or host-translated ports as remote
service identities. The fork's [socat forwarding][fork-forwarder] of management
ports is not a routing solution for arbitrary replica/service endpoints.

New lifecycle allocations freeze `publish_gateway: true` in state and publish
only n0's HTTP gateway, allowing host/WSL localhost access to Service Fabric
Explorer without a relay. Port 19080 must be free on the host. Existing
allocations without this field retain their original unpublished configuration;
resumption does not rewrite their frozen Compose inputs.

The controller rejects private subnets that overlap host routes or Docker
networks. Node containers have separate PID/network namespaces and must not
receive the Docker socket, host home, host PID namespace, or host network.
Privileged mode is retained for the validated runtime, limited to trusted local
development or an ephemeral CI VM; least privilege is not established.

The cluster is unsecured. Docker isolation does not replace authentication;
there is no public unauthenticated management endpoint or implicit production
security guarantee.

### SSH and image-store transfer

Docker create/exec/copy is sufficient for administration. G1-G4 exercised
image-store replication/download and remote native-process activation without
sshd or a shared image-store mount. **SSH is not required for the pinned,
exercised runtime path.**

The OSS [Unix copy context][unix-copy] and [remote-copy worker][ssh-copy] contain
SSH/SCP/SFTP paths. They do not override the packaged binary's observed behavior
or prove that every runtime/version/operation avoids SSH. Revalidate transfer
when changing the runtime; do not enable SSH or copy weak sample credentials
merely because those source paths exist.

## 5. Image, storage, and identity contract

The image installs SF `11.8.121.4` and sfctl `11.1.0`; the latter emits a
compatibility warning for this runtime. The exercised operations worked, but
this is not a blanket compatibility certification.

The Dockerfile pins the Ubuntu base digest, checks the downloaded runtime
package and installed Fabric executable, accepts the runtime EULA, and retains
`/opt/sf-install/servicefabric_11.8.121.4.deb` for native baseline provisioning.
Ubuntu package repositories and transitive Python dependencies are not
snapshot-pinned, so rebuilds are not byte-for-byte reproducible. The recorded
checksums establish artifact identity, not independent Microsoft
signature/published-checksum verification or a complete dependency closure.

### Three separate state layers

| Layer | Persistent content and authority |
| --- | --- |
| Controller directory, `build/linux-clusters/NAME/` | `state.json` records allocation/image/runtime/configuration identity, phase, and baseline intent. `compose.json` and bootstrap XML are frozen inputs. `events.jsonl` and `last-status.json` retain local observations. |
| Per-node external Docker volume, `/var/lib/sf-node` | `identity.json`, current deployment, application/system data, and logs. Identity binds cluster, node, IP, image ID, runtime, and volume. |
| Running SF cluster | Active XML, node versions/membership, native upgrade progress, system replicas, and health. Live observations are authoritative for operation completion. |

Controller directories/files are private; generated XML contains development
credentials. Keep state and volumes together and ignored. Deleting controller
files is not a supported teardown. Local `.paw` proof artifacts are also ignored
and must not be committed; neither the current CLI nor CI depends on them.

The controller uses a per-name filesystem lock, atomic/fsynced state writes, a
unique ownership label, and frozen-input checks. Docker volume names are
explicit/external so Compose cannot silently replace a missing retained volume.
Foreign resources, missing volumes, changed identities, and edited inputs are
errors, not authority to adopt/reset data.

### Frozen images must remain locally available

Freeze the actual Docker image ID, not just its public build tag. Retain it
under `sf-apps-node:cluster-<owner>` while cluster data exists. Lifecycle
verification exposed an image-ID lookup failure after the public tag was
rebuilt; the private tag prevents that ordinary rebuild from discarding the
image reference needed by tools and retained nodes.

`destroy` removes only that cluster's private image tag, not the public build
tag or unrelated images. Do not prune retained cluster images while down.
A new image with the same runtime version is not automatically an authorized
replacement for a frozen node image.

### Node bootstrap and restart

The entrypoint holds a node-local lock and requires an externally seeded
identity record. Fresh bootstrap transitions
`allocated -> configuring -> configured`:

1. Verify runtime, local IP, manifest, and empty allocated storage.
2. Establish required users/groups, ownership, platform paths, and permissions.
3. Validate the full manifest and run Linux deployer `Create` only on fresh data.
4. Run the required no-argument deployer and validate the current local node.
5. Supervise LTTng and `FabricHost -c -skipfabricsetup`, forwarding shutdown
   signals and reporting auxiliary/process failures.

Configured starts use the current local deployment, never fresh `Create` or a
stale mounted bootstrap. Fresh ownership adjustment does not follow runtime
symlinks and is not repeated recursively over retained application data.
`configuring` after interruption or pending `Data/TargetInformation.xml` requires
explicit recovery; automatic bootstrap retry is refused.

Never share data roots, logs, `/tmp/.service.fabric`, IPC sockets, or mutable
deployment files between containers. A new empty volume with an old node name,
or an IP change under retained data, is not an ordinary restart.

## 6. Current lifecycle behavior

The CLI operates on its own fixed three-seed topology/configuration, not
arbitrary existing clusters. It does not adopt the four-node proof environment
or reconcile manually changed topology/configuration.

| Command | Contract |
| --- | --- |
| `create NAME [--subnet CIDR]` | Default to subnet `172.30.90.0/24`, rejecting host-route/Docker-network overlaps. Verify/freeze the local node image; allocate fresh identities/secrets and external volumes; start all seeds; establish the native baseline; wait for SF readiness. Reject existing allocations/resources rather than adopt them. |
| `up NAME` | Reuse image/IP/volumes and current node deployment. Resume an observed accepted baseline instead of submitting it twice. Wait for real readiness; no implicit data reset or node restart. |
| `status NAME` | Report Docker and live SF observations. Do not start nodes. Exit 0 only for a healthy cluster, 1 for stopped/unhealthy/conflicting state. Tools may be started temporarily for SF queries. |
| `down NAME` | Stop/remove containers and bridge; preserve all node volumes and generated configuration. Record node exit codes and report non-clean shutdown even after resource removal. |
| `destroy NAME --confirm NAME` | Exact-name confirmed deletion of owned resources/data, private image tag, and generated secrets. No global prune, forced foreign-volume deletion, or recursive state-directory removal. |

Destruction leaves a small tombstone and event log. It is resumable/idempotent
after partial deletion; in-use volumes cause an explicit failure. A subsequent
fresh create under the same name uses new ownership and credentials.

### Readiness and deadlines

Success requires all three gateways to agree on the expected semantic XML,
node identities/type/address/seed flags, code/config versions, and healthy
membership. Nodes must be Up/enabled/Ok; the four checked system partitions must
each have three distinct Ready/Ok replicas and one primary. Existing applications
must be Ready/Ok. Final readiness must persist over a 30-second observation
window; Docker's running state is not sufficient.

| Budget | Scope |
| --- | --- |
| 900 seconds, configurable | Current `create`/`up` bring-up budget, including baseline work and final health window. An explicit later `up` gets a fresh readiness budget. |
| 600 seconds | Native monitored baseline upgrade timeout, separate from the surrounding lifecycle budget. |
| 600 seconds, persisted | Historical G3 end-to-end join acceptance, including its stable window and controller restarts. A future generic transaction must preserve one deadline across resume. |
| 45 minutes | Whole Linux CI job, including image build, lifecycle checks, diagnostics, and teardown attempts. |

Local failures preserve state/data for inspection. Native rollback or failure
is not success. Baseline submission intent is persisted before calling SF:
accepted/completed progress is observed; ambiguous submission with no matching
live operation is refused rather than blindly repeated.

Occasional FabricHost child stalls before Fabric starts remain unresolved.
Full retained-cluster recovery can also take several minutes while system
services recover. Do not hide either condition with resets, health-policy
bypasses, deadline extensions, or silent automatic restarts. The current CLI
offers explicit `up --restart-node NODE` recovery for n0, n1, or n2; the retained
G3 proof project must still be administered through its own Compose inputs.

## 7. Native baseline and configuration upgrades

Onebox-style bootstrap initially leaves Cluster Manager reporting code
`0.0.0.0` and empty configuration even though nodes run the expected versions.
Its placeholder `RollingForwardCompleted`/invalid-mode state is not evidence of
a completed baseline.

The [first native upgrade][first-native-upgrade] must specify **both code and
configuration versions**. The implemented controller:

1. Waits for healthy expected membership/system services and matching active XML.
2. Uploads the exact installed runtime package and current manifest through
   `fabric:ImageStore`; image-store-relative paths are used for provisioning.
3. Reuses already provisioned versions where appropriate and submits a monitored
   upgrade to the same installed code and configuration `1`.
4. Observes forward completion and stable final health.

This establishes Cluster Manager's baseline; it is not a runtime code-version
upgrade. The Linux image builder [requires the filename][linux-upgrade-filename]
`servicefabric_11.8.121.4.deb`. The original download name
`ServiceFabric.U22.11.8.121.4.deb` produced
`FABRIC_E_IMAGEBUILDER_INVALID_MSI_FILE` despite being an unchanged valid package.
Do not rename another version to bypass identity checks.

The native APIs are [Provision][provision-api] and [Upgrade][upgrade-api], exposed
by [sfctl cluster][sfctl-cluster]. `sfctl application upload` can upload an ordinary
directory; the pinned sfctl has no `sfctl store upload` command. Upload acceptance
and provisioned-version presence do not prove rollout completion.

The exercised policies use monitored mode, rollback on failure, warnings as
errors, zero unhealthy nodes/applications, 120-second replica safety,
180-second domain timeout, and 600-second native upgrade timeout. Native
[polling/health intervals][native-upgrade-polling] include 60-second defaults:
the observed three-domain rollouts took about 361 seconds and the four-domain
rollout about 481 seconds, despite a five-second configured health-stability
duration.

G2 separately proved a configuration-only `1 -> 2` change of console level
`3 -> 4`, keeping code/topology unchanged. That general upgrade procedure is
historical validation, not another exposed lifecycle command.

## 8. Dynamic join contract: proven mechanism, deferred CLI

The Windows [AddNode implementation][windows-add] supplies the ordering and
invariants, not a literal Linux deployment mechanism. Linux uses the
[Create/update path][linux-create], Docker administration, and native XML
upgrades rather than Windows services, `Configure`, SMB/Remote Registry, or
standalone JSON reconciliation through UOS.

A future generic join must implement the following transaction. The G3 private
fixed-operation driver exercised this mechanism; it is not the current CLI.

```text
Preflight -> Staged -> LocallyConfigured -> RuntimeReady
          -> ConfigProvisioned -> ConfigCommitted -> Verified
```

| Phase | Required behavior |
| --- | --- |
| Preflight | Hold the cluster operation lock; inspect authoritative active XML, all nodes including down/disabled ones, code/config versions, and upgrade state. Reject collisions, incompatible images/types/security, changed base, or concurrent upgrades. Freeze inputs and one deadline. |
| Stage private bootstrap | Copy current XML and append only fresh non-seed n3. Preserve seeds, credentials, node type, and all other settings. Mount the private copy only on n3, without rewriting existing deployments. |
| Configure/runtime-ready | Allocate fresh independent storage, configure exactly n3, and require Up/enabled with expected identity/address/type and working seed/image-store communication. Do not reactivate an existing disabled node as a substitute. |
| Provision canonical target | Recheck the frozen base, validate/upload a separate target with a fresh XML version, and provision it using an operation-specific image-store path. |
| Commit | Submit one configuration-only monitored upgrade with unchanged code. Observe actual forward completion, all-gateway XML agreement, and node-local version/settings convergence. |
| Verify/finalize | Require exact committed membership, healthy system services, working native activation/cross-node access on n3, and 30 stable seconds within the same deadline; then atomically commit inventory. |

### Bootstrap version exception

The private bootstrap retains the current XML version while appending n3.
This is a **bootstrap-only exception**, never a second canonical global artifact:

- Record base/bootstrap/target hashes and their relationship.
- Never provision the private old-version copy globally.
- Give the canonical target a fresh version.
- Require all active deployments to converge to the canonical target, while
  leaving immutable mounted bootstrap inputs untouched.

G3 demonstrated private version `2` on fresh n3 followed by canonical `3`;
n3 was absent from the initial manifest, Compose services, volumes, and observed
membership. Predeclaring a fourth container or rebuilding a static four-node
cluster is not evidence of joining a running cluster.

### Interrupted transactions

Provisioned-but-unsubmitted work resumes the frozen version/hash. After API
acceptance or caller timeout, query actual progress before resubmitting anything.
A rolled-back XML does not establish that n3's membership/data disappeared.
Preserve partially joined nodes and inspect observed state.

Before runtime startup, cleanup may remove only new, provably operation-owned
allocations where safe. Once membership is possible, preserve/quarantine the
node and volume rather than recycling their identity. A generic transaction
must expose its failed phase/node, command/API error, elapsed time, and evidence
locations, not just a Compose exit code.

Automatic destructive rollback/removal is not implemented. The Linux deployer
[does not dispatch RemoveNodeConfig][deployer-dispatch] or provide the
[Windows automatic update-removal behavior][linux-create]; do not assume a
Windows uninstall equivalent. Two tested crash boundaries do not establish
recovery after every phase, failed upgrades, partitions, or lost quorum.

## 9. Rust application integration and its limits

kvmap obtains its address through
[SF NodeContext](../crates/samples/kvmap/src/main.rs#L64). The shared formatter
supplies the [replicator](../crates/samples/kvmap/src/kvmap.rs#L231),
[RPC bind](../crates/samples/kvmap/src/kvmap.rs#L296), and
[published endpoint](../crates/samples/kvmap/src/kvmap.rs#L326), using SF-allocated
ports. RPC resolves IP/FQDN before binding; errors propagate through replica
Open rather than publishing a fallback.

The first G4 chunk demonstrated Linux/IPv4 runtime/COM activation and
cross-node replication. Formatter tests cover IPv6, hostnames, and loopback,
but those other runtime configurations were not certified. Wildcard native
replicator listeners were observed; no wildcard/loopback service address was
advertised.

The private `KvMapG4` package requested three replicas/minimum two on n1/n2/n3.
Public sample defaults remain two/minimum one. The existing packager preserves
Linux executable mode and the manifest's `kvmap.exe` filename.

Binary compatibility is separate from membership: the local host has glibc
2.39 and nodes 2.35. G4 built offline inside Ubuntu 22.04, with a builder-only
`bfd` linker override; its ELF required at most glibc 2.34. A future Linux app
CI build must not assume a binary built on the Ubuntu 24.04 host will run inside
the older runtime image. The current Linux lifecycle job does not build/deploy
kvmap.

**Replication is not crash-safe durability.**
[SingleDB](../crates/samples/kvmap/src/data.rs#L135) writes `lsn.txt` and `data.txt`
separately with ordinary [file writes](../crates/samples/kvmap/src/data.rs#L117),
without atomic commit/fsync. Persistent node volumes and `HasPersistedState=true`
do not strengthen that contract. Application-primary failover, acknowledged-data
recovery, state-provider LSN reconstruction, and cancellation semantics remain
outside completed acceptance; further experiments are deferred.

## 10. CI and resource expectations

The [Linux job](../.github/workflows/ci.yaml) runs independently of Windows on an
Ubuntu 24.04 x86-64 VM for every PR and push to `main`. It uses a local Docker
daemon directly, not a job container, remote daemon, or `ubuntu-slim` runner.
The node operating system remains pinned Ubuntu 22.04 inside Docker.

Its lifecycle is:

```text
runner prerequisites + Linux harness tests + node image build
  -> create three seeds/native baseline -> status
  -> down with retained data -> up -> status
  -> credential-safe diagnostics/artifact -> confirmed destroy
```

The job has a 45-minute deadline and a unique `ci-<run-id>-<attempt>` allocation.
It grants read-only repository access and disables persisted checkout
credentials. Diagnostics/upload and exact-name scoped cleanup are attempted
with `if: always()`, including after a lifecycle failure. An aborted/timed-out
hosted job still relies on the ephemeral VM being decommissioned; this is not a
persistent shared-runner cleanup guarantee.

Only the allowlisted diagnostics JSON is uploaded, for seven days. It contains
allocation phase/version/image identity, gateway/node/upgrade summaries,
container exit/OOM state, and CPU/memory observations. Generated XML, raw state,
runtime logs, environment variables, and arbitrary health/error descriptions
are excluded. Full status JSON stays in unuploaded runner-temporary files.
The current CI job does not exercise dynamic join or application fault/durability
acceptance.

Local workflow lint, Linux harness/diagnostics tests, Windows discovery skip,
and cleanup-shell fixtures passed. **The first GitHub-hosted execution is still
unverified**; local lifecycle success is not evidence of compatibility with the
hosted kernel, resource limits, or startup timing.

### Memory and disk

The running four-node validation cluster, including smoke and kvmap, measured
approximately **3.1 GiB of container memory** on 2026-10-18. This is a
steady-state snapshot, not a startup/upgrade peak or a measurement of the fresh
three-node CLI cluster.

Budget approximately **8 GiB available for Docker/cluster use**, with **16 GiB
total host RAM** preferable when Rust builds share the host. These are
development recommendations, not validated hard minima. No per-container
memory limits/reservations are configured; Docker's displayed host-capacity
limit is not a separate allocation for each node.

GitHub advertises 4 CPUs/16 GB for standard public-repository Ubuntu runners
([runner specifications][github-runners]), making that a reasonable initial
candidate, not a sizing certification. The locally built image alone is about
2.6 GiB before node volumes, build cache, and logs; disk headroom and runtime
recovery time must also be observed on CI.

## 11. Validation record

The local runtime proofs used WSL2 kernel
`6.6.87.2-microsoft-standard-WSL2`, Docker `29.8.0`, cgroup v2, privileged
containers, and SF `11.8.121.4` / sfctl `11.1.0`. They do not certify least
privilege, another runtime/kernel, multi-host resilience, or production support.

| Gate | Recorded outcome |
| --- | --- |
| G0: unchanged onebox | Baseline/process activation worked. Graceful shutdown failed: SIGKILL after 30 seconds, exit 137, not OOM. This was one five-node scale-min container, not the intended multi-container topology. |
| G1: three separate nodes | Passed healthy system replicas, image-store payload hashes across independent volumes, remote C activation on n2 at port 8080, and retained n2 restart/recreation without fresh `Create` or sshd. |
| G2: native configuration | Passed baseline at code/config `11.8.121.4 / 1`, separate config-only `1 -> 2` console-level change, and upgraded n2 restart/recreation while its mounted bootstrap remained `1`. Each native rollout took about 361 seconds. |
| Original G3 | Functional join/configuration `3`, remote n3 activation, and recovery worked, but **600-second timed acceptance failed**: only 28.96 of the required 30 stable seconds. Later read-only recovery did not extend or relabel that attempt. |
| Corrected G3 retest | **Passed in 574.94 seconds**, including 33.15 stable seconds, under the unchanged 600-second deadline. Both abrupt controller exits were repeated; exactly one upgrade submission was observed. Native four-domain rollout took about 481.46 seconds. |
| G4 first chunk | Passed Linux Rust activation and ten primary writes read identically from all three replicas, LSNs 1-10, UTF-8 and 32,777-byte payload, matching file/executable hashes, and over 31 seconds stable health. No failover or acknowledged-data recovery acceptance. |
| Reusable lifecycle | Passed fresh baseline/readiness, status, retained-data down/up with unchanged volume creation times/full node identities, public-image retag tolerance, and scoped/idempotent destroy. An explicit 360-second resume attempt failed; default-budget retry converged after 467.63 seconds total from retained shutdown. |
| Hosted CI | Job implemented; no hosted result recorded. |

### Failures retained rather than hidden

- G0's disposable launcher deletes its data root and did not forward shutdown
  correctly. Never resume it against retained data. The replacement entrypoint
  and G1 clean-stop observations are separate results.
- G1 required fresh deployer output ownership for sfuser and a smoke port outside
  22001-23000. Image/platform dependencies and container-sized LTTng were fixed
  in the dedicated image/entrypoint. Native-process operation still produced
  retained missing-rule/boot-time diagnostics; nested-container DNS is disabled.
- G2's first provisioning failure was the Linux package filename rule, not
  evidence that validation needed bypassing. The checked unchanged artifact
  provisioned successfully after correct naming.
- Original G3 first used unsupported `docker compose create --no-deps`, then
  had a verifier bug for omitted `IsSeedNode=false`. Fixes retained the original
  deadline; the remaining 28.96-second window still failed.
- Corrected G3's baseline n2 startup stalled in a FabricHost child before Fabric
  started; its first stop exited 1. Retained restart recovered it, without a
  reset or policy bypass. This occurred before the join clock and does not
  become a clean cold-start result because the later join passed.
- A later ordinary bring-up had the same class of n3 activation stall and
  recovered after a targeted retained restart. This was operational readiness,
  not a G4 post-write recovery/durability test.
- Lifecycle verification first exposed loss of a frozen image reference after
  rebuilding the public tag. The per-cluster retained tag fixed that path.
  A shortened resume budget also expired; its explicit retry was recorded
  separately and did not silently restart nodes, resubmit an upgrade, or reset
  data.

G3 interruption points were before submission and after API acceptance but
before acceptance journaling, using abrupt exit 86. They do not establish all
journal-phase recovery or rollback behavior. During joined-node outages, some
short management probes timed out before seed gateways converged; do not claim
uninterrupted management availability from eventual healthy observations.

### Local evidence index

| Record | Local location |
| --- | --- |
| G0 | `.paw/validation/linux-container-cluster/g0-20261017/` |
| G1, including failed attempts | `.paw/validation/linux-container-cluster/g1-20261017/`; successful run `attempt-c/` |
| G2 | `.paw/validation/linux-container-cluster/g2-20261017/` |
| Original timed G3 failure and separate recovery | `.paw/validation/linux-container-cluster/g3-20261017/` |
| Corrected timed G3 and baseline-startup failure | `.paw/validation/linux-container-cluster/g3-retest-20261017/`; committed proof inputs in `join/` |
| G4 first chunk | `.paw/validation/linux-container-cluster/g4-20261017/`; observations completed on 2026-10-18 |
| Lifecycle verification | `build/linux-cluster-validation/lifecycle-results.json` and adjacent command logs |
| Later operational startup/recovery | Session artifacts `cluster-startup-20261018.jsonl`, `n3-startup-stall-20261018.log`, and `cluster-startup-recovery-20261018.jsonl` |

These ignored/local records contain exact commands, private inputs, hashes,
observations, and failures. They are not required runtime inputs, committed
repository fixtures, or evidence that every broader acceptance case passed.

## 12. Remaining work and explicit deferrals

| Work | Disposition |
| --- | --- |
| First hosted Linux CI execution | Next infrastructure validation: observe actual kernel compatibility, disk/memory headroom, timing, failure artifacts, and cleanup. Do not waive health/deadline failures to obtain a green job. |
| Cold-start/retained recovery reliability | Unresolved runtime activation stalls and long recovery remain known limitations. Investigate from retained diagnostics rather than silently reset/restart everything. |
| Generic journaled join | Deferred product work. Promote the proven mechanism into a reusable controller only with authoritative-state preflight, frozen inputs, one resume deadline, and phase-aware recovery. |
| Wider fault/recovery matrix | Deferred: failed/rolled-back upgrades, interrupted local bootstrap, remaining journal boundaries, sustained seed loss, partitions, and lost quorum are not generally certified. |
| Full G4 failover/durability | Deferred by current scope; not a required next experiment. Reopen only with explicit application/storage acceptance and acknowledged-data/LSN recovery checks. |
| Security and other platforms/topologies | Future, separate scope: least privilege, X.509, Azure Linux, runtime upgrades, node/seed removal or replacement, and multi-host operation. |

Completed G0-G3 proofs and lifecycle implementation are not pending plan items.
A static cluster is not a substitute for a dynamic join, a late recovery is not
a pass of an expired deadline, and healthy replicas are not a durability proof.

A multi-host extension requires routed advertised endpoints, explicit per-host
provisioning/authentication, coordinated locking, durable host/node identity,
real cross-host seed/fault domains, secure key/transfer handling, and actual
host/network/storage-failure acceptance. A host-local bridge and unsecured
single-host defaults cannot simply be promoted to that deployment.

## References and source basis

The runtime fork was inspected at
`7ad146aea4dd2d23b3886121c32175e75e024b06`; the OSS source at
`3988b4518236d2d0a37f4d8bdcdf0a2093c5a61d`. Research roots were
`/home/user1/code/service-fabric-rs-fork` and
`/home/user1/code/reference/service-fabric`; those repositories were read-only.
The current image recipe is self-contained in this repository, not a dependency
on either local checkout.

| Source finding | Design consequence |
| --- | --- |
| Fork [Compose][fork-compose] and [manifest][fork-manifest] describe one privileged five-node scale-min onebox. | Multiple SF machines require a replacement manifest/entrypoint and separate storage/namespaces, not five unchanged copies. |
| Fork [Ubuntu runtime recipe][fork-dockerfile] and [launcher][fork-deployer] install/run the pinned Linux runtime but recreate disposable state. | Reuse the package/bootstrap contract, not destructive retained-data behavior. |
| Fork [Azure Linux image][fork-al-dockerfile] uses another runtime version. | It is a separate compatibility effort, not an OS/runtime mixed into the initial cluster. |
| [Linux infrastructure][linux-infrastructure], [selection][node-selection], and [Create/update][linux-create] exist in the runtime source. | Use Linux XML and fresh `Create`; validate the packaged binary rather than assume Windows `Configure`/service semantics. |
| Windows [AddNode][windows-add] stages a private manifest before canonical reconciliation. | Reuse ordering/invariants through native XML upgrades, with the bootstrap-only version exception and explicit Linux acceptance. |

[fork-compose]: https://github.com/youyuanwu/service-fabric-rs-fork/blob/7ad146aea4dd2d23b3886121c32175e75e024b06/.devcontainer/u22/docker-compose.yml#L3
[fork-manifest]: https://github.com/youyuanwu/service-fabric-rs-fork/blob/7ad146aea4dd2d23b3886121c32175e75e024b06/.devcontainer/onebox/ClusterManifest.SingleMachineFSS.xml#L64
[fork-dockerfile]: https://github.com/youyuanwu/service-fabric-rs-fork/blob/7ad146aea4dd2d23b3886121c32175e75e024b06/.devcontainer/u22/Dockerfile#L36
[fork-al-dockerfile]: https://github.com/youyuanwu/service-fabric-rs-fork/blob/7ad146aea4dd2d23b3886121c32175e75e024b06/.devcontainer/azl3/Dockerfile#L19
[fork-deployer]: https://github.com/youyuanwu/service-fabric-rs-fork/blob/7ad146aea4dd2d23b3886121c32175e75e024b06/.devcontainer/onebox/ClusterDeployer.sh#L54
[fork-forwarder]: https://github.com/youyuanwu/service-fabric-rs-fork/blob/7ad146aea4dd2d23b3886121c32175e75e024b06/.devcontainer/port_forward.sh#L3
[linux-infrastructure]: https://github.com/microsoft/service-fabric/blob/3988b4518236d2d0a37f4d8bdcdf0a2093c5a61d/src/prod/src/managed/Api/src/System/Fabric/Management/fabricdeployer/Infrastructure.cs#L50
[node-selection]: https://github.com/microsoft/service-fabric/blob/3988b4518236d2d0a37f4d8bdcdf0a2093c5a61d/src/prod/src/managed/Api/src/System/Fabric/Management/fabricdeployer/RestartOperation.cs#L241
[address-matching]: https://github.com/microsoft/service-fabric/blob/3988b4518236d2d0a37f4d8bdcdf0a2093c5a61d/src/prod/src/managed/Api/src/System/Fabric/Management/fabricdeployer/NetworkApiHelper.cs#L104
[linux-create]: https://github.com/microsoft/service-fabric/blob/3988b4518236d2d0a37f4d8bdcdf0a2093c5a61d/src/prod/src/managed/Api/src/System/Fabric/Management/fabricdeployer/CreateOrUpdateOperation.cs#L30
[windows-add]: https://github.com/microsoft/service-fabric/blob/3988b4518236d2d0a37f4d8bdcdf0a2093c5a61d/src/prod/src/managed/Api/Dlls/Microsoft.ServiceFabric.DeploymentManager/DeploymentManagerInternal.cs#L493
[linux-schema]: https://github.com/microsoft/service-fabric/blob/3988b4518236d2d0a37f4d8bdcdf0a2093c5a61d/src/prod/src/ServiceModel/xsd/ServiceFabricServiceModel.xsd#L129
[seed-infrastructure]: https://github.com/microsoft/service-fabric/blob/3988b4518236d2d0a37f4d8bdcdf0a2093c5a61d/src/prod/src/managed/Api/src/System/Fabric/Management/fabricdeployer/ServerInfrastructure.cs#L24
[deployer-dispatch]: https://github.com/microsoft/service-fabric/blob/3988b4518236d2d0a37f4d8bdcdf0a2093c5a61d/src/prod/src/managed/Api/src/System/Fabric/Management/fabricdeployer/DeploymentOperation.cs#L138
[unix-copy]: https://github.com/microsoft/service-fabric/blob/3988b4518236d2d0a37f4d8bdcdf0a2093c5a61d/src/prod/src/Management/FileStoreService/ImpersonatedSMBCopyContext.cpp#L229
[ssh-copy]: https://github.com/microsoft/service-fabric/blob/3988b4518236d2d0a37f4d8bdcdf0a2093c5a61d/src/prod/src/Common/File.cpp#L1778
[linux-dev]: https://learn.microsoft.com/en-us/azure/service-fabric/service-fabric-get-started-linux
[sfctl-cluster]: https://learn.microsoft.com/en-us/azure/service-fabric/service-fabric-sfctl-cluster
[provision-api]: https://learn.microsoft.com/en-us/rest/api/servicefabric/sfclient-api-provisioncluster
[upgrade-api]: https://learn.microsoft.com/en-us/rest/api/servicefabric/sfclient-api-startclusterupgrade
[first-native-upgrade]: https://github.com/microsoft/service-fabric/blob/3988b4518236d2d0a37f4d8bdcdf0a2093c5a61d/src/prod/src/Management/ClusterManager/ClusterManagerReplica.cpp#L3942-L3971
[linux-upgrade-filename]: https://github.com/microsoft/service-fabric/blob/3988b4518236d2d0a37f4d8bdcdf0a2093c5a61d/src/prod/src/managed/Api/src/System/Fabric/Management/ImageBuilder/FabricProvisionOperation.cs#L345-L381
[native-upgrade-polling]: https://github.com/microsoft/service-fabric/blob/3988b4518236d2d0a37f4d8bdcdf0a2093c5a61d/src/prod/src/Management/Common/ManagementConfig.h#L73-L80
[node-seed-default]: https://github.com/microsoft/service-fabric/blob/3988b4518236d2d0a37f4d8bdcdf0a2093c5a61d/src/prod/src/ServiceModel/xsd/ServiceFabricServiceModel.xsd#L290-L304
[github-runners]: https://docs.github.com/en/actions/reference/runners/github-hosted-runners
