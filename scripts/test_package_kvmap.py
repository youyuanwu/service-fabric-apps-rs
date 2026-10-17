from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from package_kvmap import package_kvmap


class PackageKvmapTests(unittest.TestCase):
    def setUp(self):
        temporary_directory = tempfile.TemporaryDirectory(prefix="kvmap packaging ")
        self.addCleanup(temporary_directory.cleanup)
        self.root = Path(temporary_directory.name)
        self.manifests = self.root / "crates" / "samples" / "kvmap" / "manifests"
        self.package = self.root / "build" / "kvmap_root"

    def write_inputs(self, executable_name):
        inputs = {
            self.manifests / "ApplicationManifest.xml": b"application manifest",
            self.manifests
            / "KvMapServicePackage"
            / "ServiceManifest.xml": b"service manifest",
            self.root / "target" / "debug" / executable_name: b"executable",
        }
        for path, content in inputs.items():
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(content)

    def assert_package(self, result):
        self.assertEqual(result, self.package)
        expected = {
            Path("ApplicationManifest.xml"): b"application manifest",
            Path("KvMapServicePackage") / "ServiceManifest.xml": b"service manifest",
            Path("KvMapServicePackage") / "Code" / "kvmap.exe": b"executable",
        }
        actual = {
            path.relative_to(self.package)
            for path in self.package.rglob("*")
            if path.is_file()
        }
        self.assertEqual(actual, set(expected))
        for relative_path, content in expected.items():
            self.assertEqual((self.package / relative_path).read_bytes(), content)

    def test_windows_package(self):
        self.write_inputs("kvmap.exe")
        with patch("package_kvmap.sys.platform", "win32"):
            self.assert_package(package_kvmap(self.root))

    def test_non_windows_package(self):
        self.write_inputs("kvmap")
        with patch("package_kvmap.sys.platform", "linux"):
            self.assert_package(package_kvmap(self.root))

    def test_missing_executable_does_not_create_package(self):
        self.write_inputs("kvmap.exe")
        (self.root / "target" / "debug" / "kvmap.exe").unlink()
        with patch("package_kvmap.sys.platform", "win32"):
            with self.assertRaisesRegex(FileNotFoundError, "Missing packaging input"):
                package_kvmap(self.root)
        self.assertFalse(self.package.exists())

    def test_missing_manifest_does_not_create_package(self):
        for relative_path in [
            Path("ApplicationManifest.xml"),
            Path("KvMapServicePackage") / "ServiceManifest.xml",
        ]:
            with self.subTest(manifest=relative_path):
                self.write_inputs("kvmap.exe")
                (self.manifests / relative_path).unlink()
                with patch("package_kvmap.sys.platform", "win32"):
                    with self.assertRaisesRegex(
                        FileNotFoundError, "Missing packaging input"
                    ):
                        package_kvmap(self.root)
                self.assertFalse(self.package.exists())

    def test_repackaging_updates_files_and_preserves_unrelated_files(self):
        self.write_inputs("kvmap.exe")
        with patch("package_kvmap.sys.platform", "win32"):
            package_kvmap(self.root)
            unrelated = self.package / "keep.txt"
            unrelated.write_bytes(b"keep")
            executable = self.root / "target" / "debug" / "kvmap.exe"
            executable.write_bytes(b"updated executable")

            package_kvmap(self.root)

        self.assertEqual(
            (self.package / "KvMapServicePackage" / "Code" / "kvmap.exe").read_bytes(),
            b"updated executable",
        )
        self.assertEqual(unrelated.read_bytes(), b"keep")


if __name__ == "__main__":
    unittest.main()
