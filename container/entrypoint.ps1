Powershell.exe -File "C:\Program Files\Microsoft SDKs\Service Fabric\ClusterSetup\DevClusterSetup.ps1" -CreateOneNodeCluster

# Wait for shutdown
[Console]::TreatControlCAsInput = $true
$Host.UI.RawUI.FlushInputBuffer()
try {
    while ($true) {
        Write-Host "Running... Press Ctrl+C to exit."
        Start-Sleep -Seconds 1

        if ($Host.UI.RawUI.KeyAvailable) {
            $key = $Host.UI.RawUI.ReadKey("AllowCtrlC,NoEcho,IncludeKeyUp")
            if (($key.Modifiers -band [ConsoleModifiers]::Control) -and ($key.Character -eq 'c')) {
                Write-Warning "Ctrl+C detected. Exiting..."
                break
            }
        }
    }
}
finally {
    Write-Host "Cleanup actions go here."
}

# TODO: Cleanup cluster?
