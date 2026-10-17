from pathlib import Path
import shutil
import sys


def package_kvmap(root: Path) -> Path:
    manifests = root / "crates" / "samples" / "kvmap" / "manifests"
    executable = root / "target" / "debug" / (
        "kvmap.exe" if sys.platform == "win32" else "kvmap"
    )
    package = root / "build" / "kvmap_root"
    files = [
        (manifests / "ApplicationManifest.xml", package / "ApplicationManifest.xml"),
        (
            manifests / "KvMapServicePackage" / "ServiceManifest.xml",
            package / "KvMapServicePackage" / "ServiceManifest.xml",
        ),
        # The service manifest expects this name on every platform.
        (executable, package / "KvMapServicePackage" / "Code" / "kvmap.exe"),
    ]

    for source, _ in files:
        if not source.is_file():
            raise FileNotFoundError(f"Missing packaging input: {source}")

    for source, destination in files:
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, destination)

    return package


if __name__ == "__main__":
    print(f"Packaged kvmap at {package_kvmap(Path(__file__).resolve().parent.parent)}")
