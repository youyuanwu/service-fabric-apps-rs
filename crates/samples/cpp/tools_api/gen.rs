use std::{
    fs::File,
    io::{self, BufRead, Write},
};

use windows_bindgen::{bindgen, Result};
fn main() -> Result<()> {
    let sf_winmd = "./build/_deps/fabric_metadata-src/.windows/winmd/Microsoft.ServiceFabric.winmd";
    let out_file = "crates/samples/cpp/src/FabricStrings.rs";
    let log = bindgen([
        "--in",
        "./crates/samples/cpp/.windows/winmd/Microsoft.ServiceFabric.FabricStrings.winmd",
        "--in",
        sf_winmd,
        "--out",
        out_file,
        "--filter",
        "Microsoft.ServiceFabric.FabricStrings",
        "--config",
        "implement",
    ])?;
    println!("{}", log);
    let mut lines = read_file_as_lines(out_file);
    remove_namespace(&mut lines, "pub mod ServiceFabric");
    remove_namespace(&mut lines, "pub mod FabricStrings");
    write_content(out_file, lines);
    Ok(())
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
