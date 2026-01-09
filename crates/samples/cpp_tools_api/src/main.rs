use windows_bindgen::bindgen;
fn main() {
    let sf_winmd = "./build/fabric_metadata-src/.windows/winmd/Microsoft.ServiceFabric.winmd";
    let out_file = "crates/samples/cpp/src/FabricStrings.rs";
    let log = bindgen([
        "--in",
        "default",
        "--in",
        "./crates/samples/cpp/.windows/winmd/Microsoft.ServiceFabric.FabricStrings.winmd",
        "--in",
        sf_winmd,
        "--out",
        out_file,
        "--reference",
        "windows,skip-root,Windows",
        "--reference",
        "mssf_com,full,Microsoft",
        "--filter",
        "Microsoft.ServiceFabric.FabricStrings",
    ]);
    println!("{log}");
}
