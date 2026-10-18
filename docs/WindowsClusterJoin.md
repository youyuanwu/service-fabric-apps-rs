# Joining an existing Service Fabric cluster on Windows

Research date: 2026-10-17.

## Summary

For a **standalone, multi-machine Windows Server cluster**, Microsoft's documented
procedure is to run `AddNode.ps1` from the standalone deployment package **on the
machine being added**, in an elevated Windows PowerShell session. Supply the new
node's identity, an existing node type, fault/upgrade domains, and an existing
cluster's client connection endpoint. The script connects to that cluster,
obtains or accepts a runtime CAB, and calls `Add-ServiceFabricNode`. After the
machine joins, perform a cluster configuration upgrade to reconcile the managed
topology. [Microsoft's procedure][add-remove] documents both steps.

This is not the same as `Connect-ServiceFabricCluster`: connecting creates an
administrative client session; joining installs/configures a machine as a runtime
node. Nor is it an application deployment operation.

Scope:

- This document covers standalone Windows Server clusters, not Azure VM scale set
  node provisioning.
- Microsoft explicitly excludes local development clusters from this add/remove
  procedure. The OSS implementation also rejects scale-min clusters.
- gMSA-secured clusters have a separate, configuration-upgrade-driven addition
  procedure, described below.

## Evidence and version boundaries

Two implementation artifacts were inspected:

1. The local [Service Fabric OSS checkout][oss-root] at
   `/home/user1/code/reference/service-fabric`, commit
   `3988b4518236d2d0a37f4d8bdcdf0a2093c5a61d`. Source links below are pinned to
   that revision. The cited implementation files had no local modifications.
2. The actual `AddNode.ps1` in the [official standalone ZIP][package-version],
   version **11.8.121.1**, obtained through Microsoft's [download link][package].
   The ZIP was downloaded and the script read without executing it or its
   autoextractor. The script is not present in the inspected OSS checkout.
   Microsoft's [package contents reference][package-contents] identifies it as
   the script for adding a node on the current machine.

The inspected script's SHA-256 is
`b7294d280731253443a5b3f8f2d49f21d9e23d8c53aed0d13566b060235b2b46`.
The download link is mutable; use the versioned URL when reproducing these findings.

**Do not assume packaged binaries exactly match the OSS checkout.** In particular,
the packaged script passes `Force`, `NoCleanupOnFailure`,
`BypassUpgradeStateValidation`, and `FabricIsPreInstalled` to its cmdlet, but these
parameters are absent from the inspected OSS `AddNode` class. The source explains
the core deployment path, not every behavior of the current binary. Use the script
and deployment components from the same official package.

No Windows cluster was available for an end-to-end join test. The commands below
were checked against documentation and the inspected script/source, not executed.

## 1. Prepare the new machine

Follow Microsoft's [deployment preparation guide][preparation] for the runtime
release and Windows Server version you intend to use.

- Use a distinct node name and a stable IP address/FQDN reachable from every
  existing node. The new machine also needs connectivity to the existing nodes.
- Select an **existing** node type. It supplies endpoints, certificates, and
  placement/capacity settings. If a new node type is needed, introduce it through
  a configuration upgrade first, wait for completion, then add nodes of that type.
- Choose fault and upgrade domains matching the intended physical failure and
  maintenance boundaries; do not blindly reuse the example values below.
- Run as a local administrator. The standalone preparation guide requires
  appropriate administrative access to the machines involved and excludes domain
  controllers. For the standard clean-install route, remove conflicting SDK/MSI
  runtime installations; do not treat this as an instruction to clean an active
  node or discard existing node data.
- Enable the required Windows deployment services, including Remote Registry,
  SMB/file sharing, and Windows Firewall. Apply the guide's network requirements
  within the trusted deployment network; do not expose these services publicly.
- Allow the configured Service Fabric node-to-node and application ports, not
  just the management endpoint. The preparation guide also lists Windows
  SMB/Remote Registry ports 135, 137, 138, 139, and 445.
- Finish the cluster's security prerequisites **before** starting the join.
  For X.509, provision the required node/server certificates, private keys, trust,
  and access permissions for the runtime identity, plus a client certificate
  authorized to administer the cluster. These are separate roles and need not
  use the same certificate. See [certificate security][x509].
- Let cluster upgrades and initial bootstrapping complete before adding a node.
  The OSS path checks the upgrade state and rejects the pre-baseline code version
  `0.0.0.0`, even if the reported upgrade state looks complete.

The source's [add-node best-practices checks][bpa-source] cover administrator
privileges, CAB validity, deployment connectivity/RPC, domain-controller
exclusion, conflicting installations, clean-install eligibility, data drives, and
free space. These checks are not proof that all runtime communication or
certificate prerequisites are satisfied.

## 2. Run the standalone join script

Copy/download and extract the **whole** [standalone package][package] on the new
machine. Keep its deployment components alongside the script. Open Windows
PowerShell as Administrator and change to the extracted package directory.

The following common arguments describe an example new machine. Replace the
addresses, node type, and topology values with your cluster's values:

```powershell
$ErrorActionPreference = 'Stop'
$node = @{
    NodeName                         = 'win-node-05'
    NodeType                         = 'NodeType0'
    NodeIPAddressorFQDN               = '10.0.0.15'
    ExistingClientConnectionEndpoint = '10.0.0.10:19000'
    UpgradeDomain                    = 'UD1'
    FaultDomain                      = 'fd:/dc1/r0'
    TimeoutInSeconds                 = 600
    AcceptEULA                       = $true
}
```

`ExistingClientConnectionEndpoint` can identify any reachable existing cluster
node's configured **client connection endpoint**. Port 19000 is the usual example,
not a requirement; use the configured port. Do not supply an Explorer URL or an
HTTP gateway endpoint such as `https://host:19080`.

Set `AcceptEULA` only after reviewing and accepting the package's license terms.
Without it, the script prompts for acceptance.

### Certificate-secured cluster

```powershell
$serverThumbprint = '<existing-cluster-server-certificate-thumbprint>'
$clientThumbprint = '<authorized-admin-client-certificate-thumbprint>'

.\AddNode.ps1 @node -X509Credential `
    -ServerCertThumbprint $serverThumbprint `
    -StoreLocation LocalMachine `
    -StoreName My `
    -FindValueThumbprint $clientThumbprint
```

Use the actual store containing the client certificate and ensure the elevated
account can access its private key. `ServerCertThumbprint` identifies the server
certificate to trust; `FindValueThumbprint` selects the local client certificate.
The script translates the latter into the connection cmdlet's
`-FindValue ... -FindType FindByThumbprint`.

**Important documentation gap:** the certificate example in Microsoft's
[add/remove article][add-remove] omits `StoreLocation`, `StoreName`, and
`FindValueThumbprint`. All three are mandatory in the certificate parameter set
of the inspected 11.8.121.1 script. Use the complete example above.

### Windows credential-secured cluster

```powershell
.\AddNode.ps1 @node -WindowsCredential
```

This uses the executing Windows identity to connect. Prepare domain membership,
authorization, and service identities according to the cluster's Windows security
configuration. The gMSA workflow below is a distinct documented option.

### Unsecured cluster: isolated prototyping only

```powershell
.\AddNode.ps1 @node
```

Do not use this security mode for a production cluster.

### Offline or explicitly selected runtime

Stage the **runtime CAB matching the existing cluster's runtime version**, then
add its path to the chosen security-mode command. For example:

```powershell
.\AddNode.ps1 @node -WindowsCredential `
    -FabricRuntimePackagePath 'C:\SF\Runtime\cluster-runtime.cab'
```

This is a CAB file path, not the standalone ZIP, an extracted directory, or an SDK
MSI. Without an explicit path, the inspected script queries
`Get-ServiceFabricClusterUpgrade`, uses `TargetCodeVersion` to look for a CAB in
`DeploymentRuntimePackages`, and otherwise attempts an online download using
`Get-ServiceFabricRuntimeSupportedVersion`. It can refuse a version no longer
listed as supported. Stage and verify the matching CAB explicitly for controlled
or disconnected deployments.

One error message in that script recommends
`-FabricRuntimePackageOutputDirectory`; this is **not** a declared script
parameter. The accepted input parameter is `-FabricRuntimePackagePath`.

## 3. Verify runtime membership and health

The script runs its connection and add-node operation in a child `Powershell`
process. Do not assume its connection remains available in the caller's session.
For example, reconnect to a certificate-secured cluster using the same client
credentials:

```powershell
Import-Module .\DeploymentComponents\ServiceFabric.psd1
Connect-ServiceFabricCluster -ConnectionEndpoint $node.ExistingClientConnectionEndpoint `
    -X509Credential -ServerCertThumbprint $serverThumbprint `
    -StoreLocation LocalMachine -StoreName My `
    -FindType FindByThumbprint -FindValue $clientThumbprint

$joinedNode = Get-ServiceFabricNode -NodeName $node.NodeName
if ($null -eq $joinedNode -or $joinedNode.NodeStatus -ne 'Up') {
    throw "The added node is missing or is not Up."
}
$joinedNode | Format-List NodeName, NodeType, IpAddressOrFQDN, NodeStatus, HealthState
Get-ServiceFabricNodeHealth -NodeName $node.NodeName
```

For Windows security, instead connect with
`Connect-ServiceFabricCluster -ConnectionEndpoint $node.ExistingClientConnectionEndpoint -WindowsCredential`.
For an isolated unsecured cluster, omit the credential switch. See
[secure connection guidance][secure-connect].

In the inspected script:

- `TimeoutInSeconds` defaults to **300** when omitted or nonpositive. The example
  above explicitly allows 600 seconds.
- The deadline is calculated **before** `Add-ServiceFabricNode` is called; it is
  not a fresh full timeout granted after that cmdlet returns.
- The subsequent loop queries `Get-ServiceFabricNode` for the requested name,
  sleeping ten seconds between unsuccessful queries.
- Its success criterion is **a matching name exists**, not `NodeStatus = Up`,
  healthy services, or completion of the configuration upgrade.

Check node health reports and expected service placement in Service Fabric
Explorer as well. Do not treat the script's success banner as a health check.

## 4. Reconcile the managed cluster configuration

Microsoft requires a configuration upgrade after the script-based addition to
ensure configuration consistency. Do this even if the new node already reports
`Up`. In a connected administrative session:

```powershell
Get-ServiceFabricClusterConfiguration |
    Set-Content -Path .\ClusterConfig.after-join.json -Encoding UTF8
```

Edit that exported file, preserving the cluster's existing settings:

1. Ensure its `nodes` array contains every intended existing node and the new
   node exactly once. Add the following entry if it is absent.
2. Change the root `clusterConfigurationVersion` to a new version, for example
   from `1.0.0` to `1.0.1`. Do not confuse this with `apiVersion` or
   `fabricVersion`; the [validator rejects an unchanged configuration version][version-source].
3. Do not combine this reconciliation with unrelated topology/security changes.
   If several nodes were added by script, account for all of them.

```json
{
  "nodeName": "win-node-05",
  "iPAddress": "10.0.0.15",
  "nodeTypeRef": "NodeType0",
  "faultDomain": "fd:/dc1/r0",
  "upgradeDomain": "UD1"
}
```

The JSON above is a **node entry**, not a complete cluster configuration file.
Submit the edited full configuration:

```powershell
Start-ServiceFabricClusterConfigurationUpgrade `
    -ClusterConfigPath .\ClusterConfig.after-join.json
Get-ServiceFabricClusterConfigurationUpgradeStatus
Get-ServiceFabricClusterUpgrade
```

The first status command reports standalone configuration-upgrade status; the
second shows the underlying Fabric upgrade progress. Monitor until the operation
has completed successfully, then re-query membership/health and retain the final
configuration for recovery. Follow the [configuration upgrade guide][config-upgrade]
for configuration testing and upgrade constraints.

The OSS [topology validator][topology-source] explains why this is necessary:
when the Upgrade Orchestration Service's topology and the Failover Manager's
observed nodes differ after script-based additions, it requires the proposed
configuration's node set to match the observed node set.

## 5. What happens under the hood

The script is deployment orchestration, not a special runtime "join token" API.
The inspected script and OSS source support the following trace:

| Stage | Finding | Evidence |
| --- | --- | --- |
| Script bootstrap | Check elevation/EULA; extract deployment components if needed; import the bundled module in a child PowerShell process. | Packaged script, starting at lines 93, 113, 279, and 310. |
| Administrative connection | Connect to the existing cluster with the selected credentials; resolve the runtime CAB; invoke `Add-ServiceFabricNode`. | Packaged script, starting at lines 312, 347, and 409. |
| Cmdlet entry | Accept node name/type/address/domains and runtime package path. Forward through `ClusterCmdletBase.AddNode` using the connected `FabricClient`. | [AddNode.cs][cmdlet-source], [ClusterCmdletBase.cs][cmdlet-base-source]. |
| Deployment API | `DeploymentManager.AddNodeAsync` forwards to `AddNodeAsyncInternal`. Its contract describes adding the current machine to an existing multi-machine cluster. | [DeploymentManager.cs][api-source]. |
| Preconditions | Check upgrade completion and baseline state; retrieve the existing XML cluster manifest and data/log roots; run add-node validation. | [DeploymentManagerInternal.cs][internal-source], [BestPracticesAnalyzer.cs][bpa-source]. |
| Bootstrap topology | Require an existing node type and Windows Server infrastructure, reject scale-min, and add the new node to a copy of the manifest with `IsSeedNode = false`. Write the generated XML to a temporary file. | [Manifest construction][manifest-source]. |
| Installation/configuration | Configure the target machine using the CAB/XCopy path. Extract/copy installer components and CAB; call `ConfigurationDeployer.NewNodeConfiguration`. | [ConfigureNodesAsync][configure-nodes-source]. |
| Local bootstrap state | Create data/log directories, set Fabric registry settings, copy the cluster manifest, write target-installation and host settings, and install the Fabric installer service. | [ConfigureOperation.cs][configure-source], [ConfigurationDeployer.cs][configuration-api-source]. |
| Runtime startup | Start `FabricInstallerSvc`, wait for installer completion and `FabricHostSvc` to run, then call `ActivateNodeAsync` to re-enable the node. | [Add-node startup][startup-source], [installer/host checks][service-source]. |
| Join observation | Query the node list until the name appears or the script deadline expires. | Packaged script, starting at line 434. |
| Durable topology update | Submit a full JSON configuration upgrade so the managed topology incorporates the observed node. | [Microsoft procedure][add-remove], [topology validation][topology-source]. |

The existing management endpoint is therefore a way to retrieve configuration
and perform administrative operations, **not the only network address needed by
the runtime**. The source's [ServerInfrastructure][infrastructure-source] derives
seed-node votes from the manifest's seed nodes and their **cluster connection
endpoints**, separately from seed-node client connection addresses. This explains
why opening only port 19000 is insufficient and why the new machine needs the
existing cluster's manifest and node-to-node connectivity.

The OSS add-node path configures the added machine; it does not itself perform
the cluster-wide JSON configuration upgrade. The generated manifest's initial
non-seed entry also should not be mistaken for an automatic reliability-level
increase or permanent seed-node assignment.

### Relationship to New-ServiceFabricNodeConfiguration

[Microsoft's cmdlet reference][new-node] describes this lower-level command as
installing/configuring the settings needed to join using a cluster manifest; the
node joins when the Service Fabric Host Service starts. It explicitly redirects
standalone users to the `AddNode.ps1` procedure.

The OSS standalone path calls the shared
`ConfigurationDeployer.NewNodeConfiguration` implementation directly rather than
shelling out to the PowerShell cmdlet of that name. The configuration API executes
the deployer's `Configure` operation. Supplying an arbitrary manifest to the
lower-level cmdlet is not a substitute for obtaining the correct existing-cluster
configuration, preparing credentials/runtime, and reconciling the managed topology.

## 6. gMSA-specific addition

For a cluster configured with a Group Managed Service Account, Microsoft's
[documented alternative][add-remove] is:

1. Prepare the new machine, including its eligibility to use the same gMSA. The
   documentation requires the account to be an administrator on the machines.
2. Export the current cluster configuration, add the new node to `nodes`, and
   change `clusterConfigurationVersion`.
3. Run `Start-ServiceFabricClusterConfigurationUpgrade` with the full edited JSON
   and monitor completion and node health.

In this route the configuration upgrade drives provisioning, rather than merely
reconciling a prior script-based join. The OSS
[PerformAddNodeOperationGMSAAsync][gmsa-source] downloads a provisioned runtime,
generates a manifest with the added nodes, configures the missing machines, and
attempts rollback on failure. The [validator][topology-source] treats gMSA additions
differently from ordinary additions not yet known to Failover Manager.

Do not generalize this into "editing JSON alone provisions nodes" for every
standalone security configuration.

## 7. Troubleshooting checklist

| Symptom | Check |
| --- | --- |
| Cannot connect | Use an existing node's client endpoint and configured port, not the HTTP/Explorer URL. Verify reachability and client authorization/certificate selection. |
| Certificate parameters prompt unexpectedly | Supply all five certificate-mode inputs: `X509Credential`, `ServerCertThumbprint`, `StoreLocation`, `StoreName`, and `FindValueThumbprint`. |
| Upgrade/bootstrap rejection | Inspect `Get-ServiceFabricClusterUpgrade`; wait for completion and a non-pre-baseline runtime. Do not use bypass/force switches as a routine workaround. |
| Runtime download failure | Stage the correct runtime CAB and use `FabricRuntimePackagePath`; inspect the script's actual parameter block rather than the misleading output-directory error message. |
| Best-practices checks fail | Check elevation, conflicting MSI installations, Remote Registry/SMB/RPC, drives/free space, and clean-install eligibility. A partial prior attempt may need deliberate recovery, not blind retries. |
| Host runs but node never appears | Check node-to-node/seed endpoint connectivity, DNS/IP identity, certificates/private-key access, and runtime diagnostics. Local service startup alone does not establish cluster membership. |
| Script succeeds but node is down/unhealthy | Its polling checks name presence only. Query node status and health explicitly and inspect health events. |
| Later configuration upgrade rejects the node list | Re-export configuration, account for every script-added node, preserve node properties, and submit a new configuration version. |

Re-run diagnostic attempts with `-Verbose` where appropriate. The OSS deployment
manager writes deployment traces under `DeploymentTraces` in its working
directory; keep those and relevant runtime/event logs when investigating failures.
Do not clean or remove an active node as a troubleshooting shortcut.

## References

The Learn articles describe the supported operator workflow. Source permalinks
support implementation findings; packaged-script line numbers refer specifically
to the version identified above.

[add-remove]: https://learn.microsoft.com/en-us/azure/service-fabric/service-fabric-cluster-windows-server-add-remove-nodes
[package-contents]: https://learn.microsoft.com/en-us/azure/service-fabric/service-fabric-cluster-standalone-package-contents
[package]: https://go.microsoft.com/fwlink/?LinkId=730690
[package-version]: https://download.microsoft.com/download/8/3/6/836e3e99-a300-4714-8278-96bc3e8b5528/11.8.121.1/Microsoft.Azure.ServiceFabric.WindowsServer.11.8.121.1.zip
[preparation]: https://learn.microsoft.com/en-us/azure/service-fabric/service-fabric-cluster-standalone-deployment-preparation
[x509]: https://learn.microsoft.com/en-us/azure/service-fabric/service-fabric-windows-cluster-x509-security
[secure-connect]: https://learn.microsoft.com/en-us/azure/service-fabric/service-fabric-connect-to-secure-cluster
[new-node]: https://learn.microsoft.com/en-us/powershell/module/servicefabric/new-servicefabricnodeconfiguration?view=azureservicefabricps
[config-upgrade]: https://learn.microsoft.com/en-us/azure/service-fabric/service-fabric-cluster-config-upgrade-windows-server
[oss-root]: https://github.com/microsoft/service-fabric/tree/3988b4518236d2d0a37f4d8bdcdf0a2093c5a61d
[cmdlet-source]: https://github.com/microsoft/service-fabric/blob/3988b4518236d2d0a37f4d8bdcdf0a2093c5a61d/src/prod/src/managed/powershell/AddNode.cs#L11
[cmdlet-base-source]: https://github.com/microsoft/service-fabric/blob/3988b4518236d2d0a37f4d8bdcdf0a2093c5a61d/src/prod/src/managed/powershell/ClusterCmdletBase.cs#L261
[api-source]: https://github.com/microsoft/service-fabric/blob/3988b4518236d2d0a37f4d8bdcdf0a2093c5a61d/src/prod/src/managed/Api/Dlls/Microsoft.ServiceFabric.DeploymentManager/DeploymentManager.cs#L90
[internal-source]: https://github.com/microsoft/service-fabric/blob/3988b4518236d2d0a37f4d8bdcdf0a2093c5a61d/src/prod/src/managed/Api/Dlls/Microsoft.ServiceFabric.DeploymentManager/DeploymentManagerInternal.cs#L493
[bpa-source]: https://github.com/microsoft/service-fabric/blob/3988b4518236d2d0a37f4d8bdcdf0a2093c5a61d/src/prod/src/managed/Api/Dlls/Microsoft.ServiceFabric.DeploymentManager/BPA/BestPracticesAnalyzer.cs#L260
[manifest-source]: https://github.com/microsoft/service-fabric/blob/3988b4518236d2d0a37f4d8bdcdf0a2093c5a61d/src/prod/src/managed/Api/Dlls/Microsoft.ServiceFabric.DeploymentManager/DeploymentManagerInternal.cs#L578
[configure-nodes-source]: https://github.com/microsoft/service-fabric/blob/3988b4518236d2d0a37f4d8bdcdf0a2093c5a61d/src/prod/src/managed/Api/Dlls/Microsoft.ServiceFabric.DeploymentManager/DeploymentManagerInternal.cs#L823
[configure-source]: https://github.com/microsoft/service-fabric/blob/3988b4518236d2d0a37f4d8bdcdf0a2093c5a61d/src/prod/src/managed/Api/src/System/Fabric/Management/fabricdeployer/ConfigureOperation.cs#L22
[configuration-api-source]: https://github.com/microsoft/service-fabric/blob/3988b4518236d2d0a37f4d8bdcdf0a2093c5a61d/src/prod/src/managed/Api/src/System/Fabric/Management/fabricdeployer/ConfigurationDeployer.cs#L151
[startup-source]: https://github.com/microsoft/service-fabric/blob/3988b4518236d2d0a37f4d8bdcdf0a2093c5a61d/src/prod/src/managed/Api/Dlls/Microsoft.ServiceFabric.DeploymentManager/DeploymentManagerInternal.cs#L642
[service-source]: https://github.com/microsoft/service-fabric/blob/3988b4518236d2d0a37f4d8bdcdf0a2093c5a61d/src/prod/src/managed/Api/Dlls/Microsoft.ServiceFabric.DeploymentManager/DeploymentManagerInternal.cs#L1126
[infrastructure-source]: https://github.com/microsoft/service-fabric/blob/3988b4518236d2d0a37f4d8bdcdf0a2093c5a61d/src/prod/src/managed/Api/src/System/Fabric/Management/fabricdeployer/ServerInfrastructure.cs#L53
[topology-source]: https://github.com/microsoft/service-fabric/blob/3988b4518236d2d0a37f4d8bdcdf0a2093c5a61d/src/prod/src/managed/Api/Dlls/Microsoft.ServiceFabric.DeploymentManager/Common/StandaloneSettingsValidator.cs#L1007
[version-source]: https://github.com/microsoft/service-fabric/blob/3988b4518236d2d0a37f4d8bdcdf0a2093c5a61d/src/prod/src/managed/Api/Dlls/Microsoft.ServiceFabric.DeploymentManager/Common/StandaloneSettingsValidator.cs#L103
[gmsa-source]: https://github.com/microsoft/service-fabric/blob/3988b4518236d2d0a37f4d8bdcdf0a2093c5a61d/src/prod/src/managed/FabricUOS/exe/UpgradeOrchestrationMessageProcessor.cs#L475
