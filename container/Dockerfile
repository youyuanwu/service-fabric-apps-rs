FROM mcr.microsoft.com/windows/server:ltsc2022

# Download Service Fabric
ADD https://download.microsoft.com/download/b/8/a/b8a2fb98-0ec1-41e5-be98-9d8b5abf7856/MicrosoftServiceFabric.11.0.2707.1.exe C:\\temp\\ServiceFabricRuntime.exe
ADD https://download.microsoft.com/download/b/8/a/b8a2fb98-0ec1-41e5-be98-9d8b5abf7856/MicrosoftServiceFabricSDK.8.0.2707.msi C:\\temp\\ServiceFabricSDK.msi

# Disable Kernel driver install
RUN reg add "HKLM\Software\Microsoft\Service Fabric" /v DisableKernelDrivers /t REG_DWORD /d 1 /f
# Install Service Fabric
RUN C:\\temp\\ServiceFabricRuntime.exe /accepteula
RUN msiexec /i C:\\temp\\ServiceFabricSDK.msi

RUN del /f /q C:\temp\ServiceFabricRuntime.exe
RUN del /f /q C:\temp\ServiceFabricSDK.msi

WORKDIR C:\\SFCluster

ADD entrypoint.ps1 C:\\SFCluster\\entrypoint.ps1
ADD ClusterManifestTemplate.json 'C:\\SFCluster\\ClusterManifestTemplate.json'
RUN del /f /q "C:\Program Files\Microsoft SDKs\Service Fabric\ClusterSetup\NonSecure\OneNode\ClusterManifestTemplate.json"
RUN move /y C:\\SFCluster\\ClusterManifestTemplate.json "C:\Program Files\Microsoft SDKs\Service Fabric\ClusterSetup\NonSecure\OneNode\ClusterManifestTemplate.json"
# Launch the cluster.
ENTRYPOINT [ "powershell", "-File", "C:\\SFCluster\\entrypoint.ps1" ]