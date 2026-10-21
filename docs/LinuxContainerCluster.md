# Linux container-cluster lifecycle

This is an **experimental, unsecured, single-host development cluster**, not a
supported production standalone deployment. It uses three privileged Ubuntu
22.04 containers, one Service Fabric 11.8.121.4 seed node per container, a private
Docker bridge, and three independent named data volumes. An administrative
`tools` container exists only while a controller command needs cluster access;
it is not an SF node. No SSH daemon is required.

For five nodes sharing one Ubuntu container and a published native-client port,
use the [single-container onebox](UbuntuOnebox.md). Its `just onebox` commands
mirror this lifecycle; both default profiles use host HTTP port 19080 and cannot
run concurrently.

## Build and create

Use a Linux x86-64 host with Python 3.10+, `ip` from iproute2, Docker Engine, and
Docker Compose v2+. The CLI talks to a **local Linux Docker daemon** and requires
permission to create privileged containers. Do not run it against a remote
Docker context: host routes and generated bind paths are local to this checkout.
`just` is optional.

```sh
docker build -t sf-apps-node:11.8.121.4 docker/linux-cluster
python3 scripts/linux_cluster_ctl.py create dev
# Equivalently: just cluster-build
#               just cluster create dev
```

The default subnet is `172.30.90.0/24`. Override it with `--subnet CIDR` when
creating another cluster or if it overlaps an existing host route or Docker
network; overlaps are rejected, not automatically reassigned. Choose an unused
private IPv4 subnet with at least 16 addresses. The tools address is subnet
offset 10; n0, n1, and n2 use offsets 11-13. Cluster gateways listen at each
node's address on port 19080; clients on the host or attached to the bridge can
use these directly.

New clusters also publish **only n0's HTTP gateway** at `127.0.0.1:19080`.
Open <http://localhost:19080/Explorer/index.html> on the host, or from Windows
when Docker runs inside WSL with localhost forwarding enabled. No separate
`socat` relay is needed. Containers retain their private bridge networking;
native-client and application ports are not published. Keep the mapping
loopback-only because the cluster is unsecured.

Host port 19080 must be free; stop another cluster or relay using that port
before starting a new cluster. Existing allocations retain their original
unpublished gateway configuration, including after `down`/`up`; do not edit
their frozen `compose.json` to add a mapping.

The image build is self-contained: no sibling repository, experimental `.paw`
files, or cached private base image is needed. The Ubuntu image digest, SF
runtime package/checksum, Fabric executable checksum, and sfctl version are
pinned. Ubuntu dependency repositories and transitive Python dependencies are
not snapshot-pinned, so rebuilds are not byte-for-byte reproducible. Building
downloads Microsoft packages and accepts the runtime EULA. The unchanged
runtime `.deb` is retained inside the image for native baseline provisioning.

`create` freezes the image's **local immutable image ID**, retains it under a
private per-cluster tag so rebuilding the public image tag cannot discard it,
generates fresh file-store credentials, allocates identities, starts all three
seeds, and
establishes Cluster Manager's native code/configuration baseline at
`11.8.121.4 / 1`. The baseline uses the exact installed runtime package and a
monitored upgrade, not a runtime code-version change. It normally takes several
minutes. Success requires all gateways to report the exact expected XML,
Up/enabled/Ok nodes, healthy system replicas, and completed native baseline,
followed by 30 seconds of stable health. Docker's running state is not success.
No sample application or application fault test runs implicitly.

## Lifecycle commands

```sh
python3 scripts/linux_cluster_ctl.py status dev
python3 scripts/linux_cluster_ctl.py down dev
python3 scripts/linux_cluster_ctl.py up dev
python3 scripts/linux_cluster_ctl.py destroy dev --confirm dev
```

| Command | Behavior |
| --- | --- |
| `create NAME [--subnet CIDR] [--image IMAGE]` | Fresh three-seed allocation and native baseline. Defaults to subnet `172.30.90.0/24`. Rejects existing state/resources rather than adopting them. |
| `up NAME` | Recreate/start using the same volumes, identities, image ID, and deployment; wait for actual SF readiness. Resume an observed accepted baseline without submitting it twice. |
| `status NAME` | JSON Docker and live SF observations. Does not start nodes. Exits 0 only when healthy; exits 1 for stopped/unhealthy/missing/conflicting resources. |
| `down NAME` | Remove this project's containers and bridge, **preserving all node volumes and generated configuration**. |
| `destroy NAME --confirm NAME` | Remove owned containers/bridge/volumes and generated secrets. Exact-name confirmation is mandatory; no global prune, force-volume deletion, or unrelated resource removal. |

Commands serialize with a per-name filesystem lock. State lives in ignored
`build/linux-clusters/NAME/`: `state.json`, `compose.json`,
`ClusterManifest.xml`, `events.jsonl`, and `last-status.json`. Generated XML
contains development credentials; files are private and must not be committed.
Keep this directory alongside the volumes: deleting it is not teardown and
prevents safe resumption. `--state-root PATH`, before the subcommand, can select
another private root; keep that root ignored and use it consistently.

External Compose volumes prevent Compose from silently replacing a missing
retained volume. The controller checks ownership labels, node/image/volume
identities, and frozen inputs. A missing or conflicting volume is an error, not
permission to create a fresh cluster under an old identity. Retained starts use
the node's current deployment rather than running fresh `Create` again.
`down` records each node's exit code before removing containers. A forced or
failed shutdown is reported as an error even though teardown completes and all
volumes remain available for recovery.

`destroy` leaves a small destroyed-state record and operation log. It can be
repeated after an interrupted deletion; volumes still in use cause an explicit
failure. After successful destruction the same name can be created again with
new credentials and ownership. Other state files are not recursively deleted.
The private per-cluster image tag is removed; the public build tag is not pruned.
Previously created G1/G3 validation projects are **not adopted or modified**.
Do not prune retained cluster images while a cluster is down: rebuilding an
image is not authority to replace its frozen identity or discard node data.

## Failures and retained-state recovery

`create`/`up` default to a 900-second bring-up deadline, including startup,
baseline upload/provisioning/rollout, and the stable-health window. Set
`--timeout SECONDS` on either command for a different finite positive budget.
The native monitored baseline itself retains a 600-second upgrade timeout.
Timeouts/errors leave resources and state available for inspection; they do not
reset data. `up` is the explicit retry operation with a fresh readiness budget.
Full retained-cluster recovery can take several minutes even when Fabric
processes are running; do not confuse container startup with recovered SF
membership and system-service quorum.

Inspect `last-status.json`, `events.jsonl`, and the affected node's logs:

```sh
docker compose -f build/linux-clusters/dev/compose.json logs --tail 100 n2
python3 scripts/linux_cluster_ctl.py up dev --restart-node n2
```

The pinned runtime has occasionally stalled in a FabricHost child before
starting Fabric. A targeted retained-state restart recovered the validation
instances, but the cause is unresolved. The CLI never silently restarts/reset
nodes to hide this condition. `--restart-node` is explicit, repeatable, and
limited to this cluster's three nodes. Do not restart all seeds to recover a
single stalled node.

An interrupted fresh node deployment (`configuring` identity) or pending
`TargetInformation.xml` is rejected and requires investigation, not an automatic
bootstrap retry. If baseline submission was interrupted, the controller
reconciles live target progress. An accepted/completed target is observed;
ambiguous submission with no corresponding live operation is refused rather
than blindly resubmitted. Preserve the state and inspect native progress before
manual recovery, or explicitly destroy/recreate a disposable cluster.

Dynamic `join`, removal, runtime upgrades, migration/adoption, production
security, and sample-application durability validation are outside this first
CLI release. The dynamic-join proof and full architecture are recorded in
[the design](LinuxContainerClusterDesign.md); Compose scaling is not SF joining.

## Continuous integration

The `Linux cluster lifecycle` matrix entry in `.github/workflows/ci.yaml` runs separately
from the Windows build on every pull request and push to `main`. It uses an
Ubuntu 24.04 x86-64 VM directly, builds the pinned Ubuntu 22.04 node image, and
checks fresh creation/native baseline, health, retained-data down/up, resumed
health, and a surviving synthetic naming-store entry. The onebox entry runs
independently on another runner. Each entry has a
45-minute deadline and a unique cluster name per run/attempt.
It does not run joins or application fault tests.

Diagnostics and exact-name confirmed destruction are attempted with `always()`,
including after failures. A seven-day artifact contains only allowlisted
allocation, gateway/node/upgrade, container state, and memory/CPU summaries.
Generated manifests, raw state, runtime logs, environment variables, and arbitrary
health/error descriptions are not uploaded because they can contain credentials.
The Linux-only controller tests skip on Windows so the existing Windows test
discovery remains usable. Hosted-runner behavior still needs its first CI run;
the local lifecycle verification is not a hosted-runner certification.
