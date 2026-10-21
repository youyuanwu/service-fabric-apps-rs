# Rust FabricClient POC

**Status:** experimental native clear-TCP subset. Gateway ping and NameExists
have been demonstrated against the repository's Ubuntu onebox running
Service Fabric **11.8.121.4**. This is not a complete FabricClient replacement
and does not support TLS or authentication.

## 1. What is implemented

`mssf-transport` implements bounded Fabric serialization, clear frames and CRCs,
ordered native headers, client-only ListenInstance establishment, and
sequential Tokio request/reply I/O. `mssf-client` adds native gateway ping and
NameExists, with owned results and explicit errors.

Neither package calls the native Service Fabric client or COM libraries.
Neither uses HTTP to execute client operations. "Clear TCP" still carries the
**native binary Fabric protocol**, not JSON or line-based messages.

| Operation | Native behavior and evidence |
|---|---|
| Gateway ping | Sends PingRequest with an empty body and decodes gateway address, node ID, node instance, and node name. Full identity compared with independent onebox observations. |
| NameExists | Sends a parsed ten-field native URI object; independently established existing/absent names returned true/false on the same connection. |
| Native faults/errors | Retains native signed error codes and available diagnostics; source-derived/model-peer coverage, not a claim of live error-path qualification. |
| Stream lifecycle | Bounded deadlines, fragmentation, partial writes, correlation, cancellation, and close; offline model-peer coverage. |

The client advertises protocol **1.1**. The demonstrated gateway replies with
protocol **1.2**. Coverage is qualified only for the observed runtime,
clear-TCP profile, and these operations, not all Service Fabric releases.

## 2. Run against a disposable onebox

Use the repository's Rust toolchain. A native SDK or C++ build is unnecessary.
The [Ubuntu onebox guide](../UbuntuOnebox.md) supplies Docker prerequisites.
Run from the repository root:

```sh
just onebox-build
just onebox create poc

cargo run -p mssf-client --example onebox -- \
  --clear 127.0.0.1:19000 ping
```

The example prints the gateway's advertised address and full node identity.
The advertised address can be the container's private address rather than the
loopback endpoint used to dial it.

Prepare a synthetic name through **management only**, on your own disposable
cluster:

```sh
curl --noproxy '*' --fail --silent --show-error \
  --max-time 30 --request POST --header 'Content-Type: application/json' \
  --data '{"Name":"fabric:/rust-poc-existing"}' \
  'http://127.0.0.1:19080/Names/$/Create?api-version=6.0'

cargo run -p mssf-client --example onebox -- \
  --clear 127.0.0.1:19000 name-exists fabric:/rust-poc-existing

cargo run -p mssf-client --example onebox -- \
  --clear 127.0.0.1:19000 name-exists fabric:/rust-poc-absent
```

The naming results should be true and false respectively, provided the second
name has not been created. A native failure never becomes false.
Port **19000** is native TCP; **19080** is HTTP management, not a fallback.
The `--clear` flag is mandatory. Endpoints are IP socket addresses, including
bracketed IPv6, not DNS names or URLs.

The default connection/per-operation timeout is 30 seconds. Override it with
`--timeout-seconds 5`; only finite positive durations of at least 100 ns that
fit native TimeSpan and the monotonic clock are accepted. Errors use a nonzero
exit code.

Remove only your disposable allocation when finished:

```sh
just onebox destroy poc --confirm poc
```

## 3. Rust API and lifecycle

```rust,no_run
use mssf_client::{FabricClient, DEFAULT_TIMEOUT};

# async fn example() -> mssf_client::Result<()> {
let endpoint = "127.0.0.1:19000".parse().expect("IP socket endpoint");
let mut client = FabricClient::connect_clear(endpoint, DEFAULT_TIMEOUT).await?;
let gateway = client.ping().await?;
let exists = client.name_exists("fabric:/app/service").await?;
client.close().await?;
# Ok(())
# }
```

The handle is mutable and **single-flight**. Sequential operations reuse one
connection; there is no cloneable concurrent multiplexer, automatic reconnect,
retry, or replay.

Each operation has one local monotonic deadline covering encoding, sending,
control dispatch, receiving, and result decoding. Replies match both message
GUID and index. Unrelated, duplicate, late, and push replies cannot complete
another operation. More than 1,024 interleaved controls/unrelated messages
terminates that exchange.

The in-flight future owns the socket. Cancellation, timeout, or transport/
envelope failure closes it rather than leaving a partially written frame on
a reusable stream. Malformed success data also closes it. An ordinary complete
native failure remains an error but does not itself poison the stream.
Invalid local naming input is rejected without sending.
`close()` is explicit and idempotent; dropping the handle releases the socket.
There is no client-owned background task.

## 4. Supported inputs and bounds

Naming input is literal lowercase `fabric:/` followed by nonempty ASCII
`[A-Za-z0-9_-]+` segments separated by single slashes. Path case and spelling
are preserved. The entire input is limited to 4,096 bytes.

Root names, authorities, Unicode, escapes, query/fragment, whitespace, dot
segments, other punctuation, and trailing/repeated slashes are unsupported
and return an explicit input error. This is a deliberate subset, not a full
native NamingUri parser.

| Resource | Limit |
|---|---:|
| Entire frame | 4 MiB |
| Native header area | 65,535 bytes |
| Header records, including deleted records | 4,096 |
| Object extent, excluding its leading marker | 4 MiB, additionally constrained by the containing frame |
| Object nesting | 32 |
| String | 32,768 UTF-16 code units |
| Byte-array/collection count helper | 4,096 |

The decoder skips unknown/deleted header records by their bounded record
lengths. Effective common headers use the last visible occurrence; first
typed version lookup remains first-match. Supported appended values/scopes
are typed-skipped, not found by searching for delimiter bytes.
Unsupported trailing metadata, no-copy extensions, or object flags fail
explicitly. General native forward compatibility is not claimed.

## 5. Offline and native qualification

Ordinary checks need neither onebox nor native client libraries:

```sh
cargo test --locked -p mssf-transport -p mssf-client
cargo clippy --locked -p mssf-transport -p mssf-client --all-targets -- -D warnings
cargo doc --locked -p mssf-transport -p mssf-client --no-deps
```

Transport fixture manifests distinguish source-derived constructions from
reviewed native responses. The native control/ping regression fixtures were
captured from a synthetic unsecured onebox exchange made by an independent
socket probe, not generated by the Rust encoder. Only known nonsemantic
object padding was zeroed and CRCs recomputed; each normalization is recorded.
Rust round trips and constructed vectors are not native interoperability proof.

Both live targets are ignored by default. On a fresh disposable local onebox,
the Rust `onebox_setup` helper uses test-only `reqwest` HTTP calls to prepare
synthetic names and independently observe gateway identity:

```sh
mkdir -p build/poc-native
export MSSF_TEST_CONFIG="$PWD/build/poc-native/config.json"
export MSSF_TEST_REPORT="$PWD/build/poc-native/report.json"
cargo test --locked -p mssf-client --test onebox_setup -- --ignored
cargo test --locked -p mssf-client --test onebox_clear -- --ignored
```

Setup uses the published local HTTP gateway at `127.0.0.1:19080` and records
the first native gateway at `127.0.0.1:19000` / N0010. Run setup once on a fresh
disposable onebox; an existing fixture name or unexpected HTTP result is an
error. Proxy use and redirects are disabled. No additional CI job is needed.
`reqwest` has JSON support only, with default/TLS features disabled, and is a
dev-dependency rather than a production client dependency.

For a separately prepared environment, the native `onebox_clear` target still
accepts a private JSON file with the following fields:

| Field | Meaning |
|---|---|
| `profile` | Exactly `"clear"`. |
| `gateway` | Native IP socket endpoint, such as `"127.0.0.1:19000"`. |
| `runtime_version` | Actual code version independently observed from the target. |
| `existing_name`, `absent_name` | Distinct supported names independently confirmed existing/absent. |
| `expected_node_name` | Gateway node name, e.g. `"N0010"` for the published first gateway. |
| `expected_node_id` | Canonical lowercase hexadecimal native node ID. |
| `expected_node_instance` | Numeric native node instance. |
| `report_path` | Absolute path for a sanitized JSON result, in an existing private directory. |

Use HTTP `GET /Nodes?api-version=6.0` for independent node/version observations
(`CodeVersion`, `Name`, `Id.Id`, and numeric `InstanceId`). Verify fixture state
independently using `GET /Names/NAME?api-version=6.0`; absent is HTTP 404.
HTTP is fixture preparation/observation only. Native results are obtained
exclusively through the Rust native connection.

Use **absolute paths** for both the configuration file and report. Cargo runs
integration tests from the package directory:

```sh
MSSF_TEST_CONFIG="$PWD/build/poc-native-config.json" \
  cargo test --locked -p mssf-client --test onebox_clear -- --ignored
```

The selected test fails on missing/wrong configuration, inaccessible native
gateway, identity mismatch, or unmet naming prerequisites. It writes a result
only after ping, true/false naming results on the same connection, and explicit
close pass. Runtime metadata comes from the independent preparation; the ping
body does not advertise the server's code version.

CI runs native-free Linux checks, then uses the Rust setup helper and configured
native target inside its existing disposable onebox lifecycle job. Only the
success report is uploaded, never the private configuration, raw runtime logs,
or credentials.

## 6. Deliberate exclusions

No TLS/rustls, authentication, HTTP operation fallback, Windows-specific
integration, native client wrapper, full URI/API coverage, service resolution,
notifications, production failover, or concurrent request handling is provided.
The onebox is privileged and unsecured: keep it local with loopback-only
published endpoints. Do not use this profile on a production or shared cluster.

The [production design](design/FabricClient.md) and
[broader implementation plan](design/ImplementationPlan.md) remain separate.
This POC does not complete their production lifecycle or TLS requirements.
