# Guest-side provisioning, run once over SSH by build-image.sh.
# Installs the two things the archive runner needs on Windows:
#   - cargo-nextest.exe  (executes tests from the cross-built archive; no Rust
#     toolchain required on the guest)
#   - Node.js LTS        (the snark node-subprocess tests shell out to `node`,
#     matching the setup-node step in the CI Windows lane)
# Both go under C:\facet and are added to the machine PATH so SSH sessions
# (which don't load an interactive profile) can find them.
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'  # Invoke-WebRequest is glacial with the progress bar

$root = 'C:\facet'
$bin  = Join-Path $root 'bin'
New-Item -ItemType Directory -Force -Path $bin | Out-Null

Write-Host '==> installing cargo-nextest.exe'
$nextestZip = Join-Path $env:TEMP 'nextest.zip'
Invoke-WebRequest -Uri 'https://get.nexte.st/latest/windows' -OutFile $nextestZip
Expand-Archive -Path $nextestZip -DestinationPath $bin -Force
Remove-Item $nextestZip

Write-Host '==> installing Node.js LTS'
$idx = Invoke-RestMethod -Uri 'https://nodejs.org/dist/index.json'
$lts = ($idx | Where-Object { $_.lts } | Select-Object -First 1).version
$nodeDir = Join-Path $root "node-$lts-win-x64"
if (-not (Test-Path $nodeDir)) {
    $nodeZip = Join-Path $env:TEMP 'node.zip'
    Invoke-WebRequest -Uri "https://nodejs.org/dist/$lts/node-$lts-win-x64.zip" -OutFile $nodeZip
    Expand-Archive -Path $nodeZip -DestinationPath $root -Force
    Remove-Item $nodeZip
}
Write-Host "    node $lts"

# Prepend our tools to the *machine* PATH, de-duplicated.
$machinePath = [Environment]::GetEnvironmentVariable('Path', 'Machine')
$wanted = @($bin, $nodeDir)
$parts = $machinePath -split ';' | Where-Object { $_ -and ($wanted -notcontains $_) }
$newPath = (($wanted + $parts) -join ';')
[Environment]::SetEnvironmentVariable('Path', $newPath, 'Machine')

Write-Host '==> versions'
& (Join-Path $bin 'cargo-nextest.exe') --version
& (Join-Path $nodeDir 'node.exe') --version

Write-Host '==> guest provisioning complete'
