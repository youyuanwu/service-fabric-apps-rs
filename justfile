set windows-shell := ["powershell.exe", "-NoLogo", "-NoProfile", "-Command"]

build: build-rust package

build-rust:
    cargo build --workspace

package:
    python "{{join(justfile_directory(), 'scripts', 'package_kvmap.py')}}"
