// ------------------------------------------------------------
// Copyright (c) Microsoft Corporation.  All rights reserved.
// Licensed under the MIT License (MIT). See License in the repo root for
// license information.
// ------------------------------------------------------------

use windows_bindgen::bindgen;

fn main() {
    let sf_winmd = "./build/_deps/fabric_metadata-src/.windows/winmd/Microsoft.ServiceFabric.winmd";
    let out_file = "crates/libs/c/src/ReliableCollectionRuntime.rs";
    bindgen([
        "--in",
        "default",
        "--in",
        "./.windows/winmd/Microsoft.ServiceFabric.ReliableCollectionRuntime.winmd",
        "--in",
        sf_winmd,
        "--out",
        out_file,
        "--filter",
        "Microsoft.ServiceFabric.ReliableCollectionRuntime",
        "--no-allow",
        "--reference",
        "windows,skip-root,Windows",
        "--reference",
        "mssf_com,full,Microsoft",
    ])
    .unwrap();
}
