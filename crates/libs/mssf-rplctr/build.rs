// ------------------------------------------------------------
// Copyright (c) Microsoft Corporation.  All rights reserved.
// Licensed under the MIT License (MIT). See License in the repo root for
// license information.
// ------------------------------------------------------------

use std::{env, path::Path};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let package_root = env::var("CARGO_MANIFEST_DIR").unwrap();

    // repo root.
    let abs_dir = Path::new(&package_root)
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let mut exe = abs_dir.to_path_buf();
    if cfg!(windows) {
        exe.push("build/_deps/protoc-src/bin/protoc.exe");
    } else {
        exe.push("build/_deps/protoc-src/bin/protoc");
    }
    std::env::set_var("PROTOC", exe);
    // generate kvstore for grpc
    tonic_build::compile_protos("proto/rplctr.proto")?;

    Ok(())
}
