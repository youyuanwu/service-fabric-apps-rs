//! Thin native-profile command runner; scenario behavior lives in the library.
use std::path::PathBuf;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 5 {
        eprintln!(
            "usage: mssf-test-host NATIVE_DIRECTORY DEPENDENCY_DIRECTORY APP_EXECUTABLE RUN_PARENT"
        );
        std::process::exit(2);
    }
    let options = mssf_test_host::Options {
        native_directory: PathBuf::from(&args[1]),
        dependency_directory: PathBuf::from(&args[2]),
        executable: PathBuf::from(&args[3]),
        run_parent: PathBuf::from(&args[4]),
    };
    if let Err(error) = mssf_test_host::bootstrap(options) {
        eprintln!("native scenario FAILED: {error}");
        std::process::exit(1);
    }
}
