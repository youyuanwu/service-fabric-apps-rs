use crate::{wire, HostError, Result};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub struct Deployment {
    pub root: PathBuf,
    pub deployment: PathBuf,
    pub work: PathBuf,
    pub code: PathBuf,
    pub config: PathBuf,
}

impl Deployment {
    pub fn create(parent: &Path, executable: &Path) -> Result<Self> {
        let parent = parent.canonicalize()?;
        let root = parent.join(format!("native-host-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root)?;
        let deployment = root.join("deployment");
        let application = deployment.join(wire::APP_ID);
        let code = application.join("NativeService.Code.1.0");
        let work = root.join("node-work");
        for directory in [
            &code,
            &work,
            &application.join("work"),
            &application.join("log"),
            &application.join("temp"),
            &root.join("native-data"),
        ] {
            fs::create_dir_all(directory)?;
        }
        fs::copy(executable, code.join("stateless-test-app"))?;
        let namespace = "http://schemas.microsoft.com/2011/01/fabric";
        let code_xml = r#"<CodePackage Name="Code" Version="1.0"><EntryPoint><ExeHost><Program>stateless-test-app</Program><WorkingFolder>CodePackage</WorkingFolder></ExeHost></EntryPoint></CodePackage>"#;
        let types_xml = r#"<ServiceTypes><StatelessServiceType ServiceTypeName="NativeStatelessType" /></ServiceTypes>"#;
        fs::write(
            application.join("App.1.0.xml"),
            format!(
                r#"<ApplicationPackage xmlns="{namespace}" ApplicationId="NativeTestHost_App1" ApplicationTypeName="NativeTestHost" NameUri="fabric:/NativeTestHost" RolloutVersion="1.0"><DigestedEnvironment RolloutVersion="1.0"><Principals /><Policies /><Diagnostics /></DigestedEnvironment><DigestedCertificates RolloutVersion="1.0" /></ApplicationPackage>"#
            ),
        )?;
        fs::write(
            application.join("NativeService.Package.Current.xml"),
            format!(
                r#"<ServicePackage xmlns="{namespace}" Name="NativeService" ManifestVersion="1.0" RolloutVersion="1.0"><DigestedServiceTypes RolloutVersion="1.0">{types_xml}</DigestedServiceTypes><DigestedCodePackage RolloutVersion="1.0">{code_xml}</DigestedCodePackage><DigestedResources RolloutVersion="1.0" /><Diagnostics /></ServicePackage>"#
            ),
        )?;
        fs::write(
            application.join("NativeService.Manifest.1.0.xml"),
            format!(
                r#"<ServiceManifest xmlns="{namespace}" Name="NativeService" Version="1.0">{types_xml}{code_xml}<Resources /></ServiceManifest>"#
            ),
        )?;
        // No declared static endpoint or principal: these files still must exist.
        // The service binds port zero during Open and returns its actual address.
        fs::write(application.join("NativeService.Endpoints.txt"), "")?;
        fs::write(application.join("Application.Sids.txt"), "")?;
        let config = root.join("FabricConfig.cfg");
        fs::write(
            &config,
            "[Trace/Console]\nLevel=4\n[Trace/Etw]\nLevel=0\n[Trace/File]\nLevel=0\n",
        )?;
        Ok(Self {
            root,
            deployment,
            work,
            code,
            config,
        })
    }
}

pub fn preflight(runtime: &Path, app: &Path) -> Result<()> {
    if !cfg!(target_os = "linux") || !cfg!(target_arch = "x86_64") {
        return Err(HostError(
            "only Linux x86_64 native profile is supported".into(),
        ));
    }

    if fs::read_to_string(runtime.join("ClusterVersion"))?.trim() != "11.8.121.4" {
        return Err(HostError(
            "unsupported native runtime: require 11.8.121.4".into(),
        ));
    }
    for name in ["libFabricRuntime.so", "libFabricCommon.so"] {
        if !runtime.join(name).is_file() {
            return Err(HostError(format!("missing native library {name}")));
        }
    }
    if !app.is_file() {
        return Err(HostError("native test app executable is missing".into()));
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_bundle_is_unique_and_has_required_native_inputs() {
        let first =
            Deployment::create(&std::env::temp_dir(), &std::env::current_exe().unwrap()).unwrap();
        let second =
            Deployment::create(&std::env::temp_dir(), &std::env::current_exe().unwrap()).unwrap();
        assert_ne!(first.root, second.root);
        let application = first.deployment.join(wire::APP_ID);
        for file in [
            "App.1.0.xml",
            "NativeService.Package.Current.xml",
            "NativeService.Manifest.1.0.xml",
            "NativeService.Endpoints.txt",
            "Application.Sids.txt",
        ] {
            assert!(application.join(file).is_file());
        }
        assert!(fs::read_to_string(&first.config)
            .unwrap()
            .contains("[Trace/Console]"));
        fs::remove_dir_all(first.root).unwrap();
        fs::remove_dir_all(second.root).unwrap();
    }

    #[test]
    fn missing_native_profile_fails_before_launch() {
        assert!(preflight(Path::new("/no-such-native-profile"), Path::new("/bin/true")).is_err());
    }
}
