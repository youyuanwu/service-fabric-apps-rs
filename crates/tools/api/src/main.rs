// ------------------------------------------------------------
// Copyright (c) Microsoft Corporation.  All rights reserved.
// Licensed under the MIT License (MIT). See License in the repo root for
// license information.
// ------------------------------------------------------------

use std::{
    fs::File,
    io::{self, BufRead, Write},
};

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
    ]);
    let mut lines = read_file_as_lines(out_file);
    remove_namespace(&mut lines, "pub mod Microsoft");
    remove_namespace(&mut lines, "pub mod ServiceFabric");
    remove_namespace(&mut lines, "pub mod ReliableCollectionRuntime");
    write_content(out_file, lines);
}

fn read_file_as_lines(path: &str) -> Vec<String> {
    let r = File::open(path).unwrap();
    let reader = io::BufReader::new(r);
    // process each line and skip the lines targeted
    reader.lines().map(|x| x.unwrap()).collect::<Vec<String>>()
}

fn remove_namespace(lines: &mut Vec<String>, skip_str: &str) {
    lines.retain(|line| {
        if line.contains(skip_str) {
            return false;
        }
        true
    });
    lines.pop();
}

fn write_content(path: &str, lines: Vec<String>) {
    File::create(path)
        .unwrap()
        .write_all(lines.join("\n").as_bytes())
        .unwrap();
}
