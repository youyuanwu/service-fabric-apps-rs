# Service Fabric Rust Apps
Various service fabric rust apps.

# Apps and libs
* [kvmap](crates\samples\kvmap) an key value store using SF default replicator. Still work in progress.
* [kvstore](crates\samples\kvstore) app using default SF key value store implementation in FabricRuntime.
* [ext](crates\libs\mssf-ext) lib that extends mssf-core crate with various support including default replicator.

# Build
Install Rust (using the toolchain pinned in `rust-toolchain.toml`), [just](https://just.systems), Python 3, and Protocol Buffers (`protoc`).

Service Fabric dependencies use the published `mssf` 0.10 release family.
Tokio-based Service Fabric executors require a multi-threaded Tokio runtime.

```ps1
just build
```

`just` also runs the build by default. It compiles the workspace and packages `kvmap`
under `build/kvmap_root`, ready for the deployment script below.
Use `just build-rust` to compile without packaging, or `just package` to repackage
an existing debug build. Packaging uses only the Python standard library.

# Run Example on SF-onebox
```ps1
.\scripts\kvmap_ctl.ps1 -Action Add
```

# License
MIT
