# Service Fabric Rust Apps
Various service fabric rust apps.

# Reliable Collections Support
Rust support for [reliable collection](https://learn.microsoft.com/en-us/azure/service-fabric/service-fabric-reliable-services-reliable-collections) in Service Fabric.

Experimental and APIs are subject to change.

Currently only Winodws is supported.

* See [rcstore](crates\samples\rcstore) for usage.
* [sfrc-core](crates\libs\core) reliable collection bindings.

# Other apps and libs
* [kvmap](crates\samples\kvmap) an key value store using SF default replicator. Still work in progress.
* [kvstore](crates\samples\kvstore) app using default SF key value store implementation in FabricRuntime.
* [ext](crates\libs\mssf-ext) lib that extends mssf-core crate with various support including default replicator.

# Build
```ps1
cmake . -B build
cmake --build build
cargo build --all
```

# Run Example on SF-onebox
```ps1
.\scripts\kvstore_ctl.ps1 -Action Add
```

# License
MIT

