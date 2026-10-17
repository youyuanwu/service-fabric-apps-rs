# Service Fabric Rust Apps
Various service fabric rust apps.

# Apps and libs
* [kvmap](crates\samples\kvmap) an key value store using SF default replicator. Still work in progress.
* [kvstore](crates\samples\kvstore) app using default SF key value store implementation in FabricRuntime.
* [ext](crates\libs\mssf-ext) lib that extends mssf-core crate with various support including default replicator.

# Build
Install Rust (using the toolchain pinned in `rust-toolchain.toml`), CMake 3.28 or newer, and Protocol Buffers (`protoc`).

Service Fabric dependencies use the published `mssf` 0.10 release family.
Tokio-based Service Fabric executors require a multi-threaded Tokio runtime.

```ps1
cmake . -B build
cmake --build build
cargo build --all
```

# Run Example on SF-onebox
```ps1
.\scripts\kvmap_ctl.ps1 -Action Add
```

# License
MIT
