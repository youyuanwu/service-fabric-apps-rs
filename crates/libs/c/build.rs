// ------------------------------------------------------------
// Copyright (c) Microsoft Corporation.  All rights reserved.
// Licensed under the MIT License (MIT). See License in the repo root for
// license information.
// ------------------------------------------------------------

use std::{env, path::Path};

fn main() {
    if cfg!(windows) {
        // add link dir for fabric support libs. This is propagated to downstream targets
        let dir = String::from("build");
        let package_root = env::var("CARGO_MANIFEST_DIR").unwrap();
        let abs_dir = package_root + "\\..\\..\\..\\" + &dir;
        println!(
            "cargo:rustc-link-search=native={}",
            Path::new(&abs_dir).display()
        );
    } else if cfg!(unix) {
        // Add link dir for fabric libs on linux.
        let dir = String::from("/opt/microsoft/servicefabric/bin/Fabric/Fabric.Code/");
        println!("cargo:rustc-link-search={}", Path::new(&dir).display());
        println!("cargo:rustc-link-lib=dylib=ReliableCollectionRuntime")
    }
}
