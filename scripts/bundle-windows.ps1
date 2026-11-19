# Packages a compiled openDJ for Windows as dist/opendj-VERSION-windows-x64.zip
# (portable: unzip anywhere and run openDJ.exe) and
# dist/openDJ-VERSION-windows-x64-setup.exe (per-user install, no admin).
# Usage: scripts/bundle-windows.ps1 BINARY VERSION
# Needs Inno Setup 6 (installed with Chocolatey when missing). When
# SIGN_COMMAND is set, it runs with the file to sign appended, for
# openDJ.exe and then for the setup.
param(
  [Parameter(Mandatory)] [string] $Binary,
  [Parameter(Mandatory)] [string] $Version
)
$ErrorActionPreference = "Stop"

function Sign($file) {
  if ($env:SIGN_COMMAND) {
    Invoke-Expression "$env:SIGN_COMMAND `"$file`""
    if ($LASTEXITCODE) { throw "signing $file failed" }
  }
}

# The main thread's stack reserve, from the exe's PE header: crates/app/build.rs
# sets 8 MB, as macOS and Linux give. With Windows' default 1 MB the app
# crashes with a stack overflow at startup.
$fs = [IO.File]::OpenRead($Binary)
$head = New-Object byte[] 4096
[void]$fs.Read($head, 0, $head.Length)
$fs.Close()
$pe = [BitConverter]::ToInt32($head, 0x3C)
$reserve = [BitConverter]::ToUInt64($head, $pe + 24 + 72)
if ($reserve -lt 8MB) { throw "$Binary reserves $reserve bytes of stack, not 8 MB" }

New-Item -ItemType Directory -Force dist | Out-Null
$stage = "dist/openDJ"
if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
New-Item -ItemType Directory $stage | Out-Null
Copy-Item $Binary "$stage/openDJ.exe"
# The helper that runs audio plugins, built next to the app's binary.
$helper = Join-Path (Split-Path $Binary) "opendj-plugin-host.exe"
if (Test-Path $helper) {
  Copy-Item $helper "$stage/opendj-plugin-host.exe"
} else {
  Write-Warning "no $helper, so this build can't run audio plugins"
}
Copy-Item -Recurse res/controllers "$stage/controllers"
# The guided tour's demo tracks, once they are in the repository.
if (Test-Path res/demo) { Copy-Item -Recurse res/demo "$stage/demo" }
Copy-Item README.md, NOTICE "$stage/"
Copy-Item LICENSE "$stage/LICENSE-GPL2.txt"
# The ASIO SDK built in is GPLv3, so the Windows build as a whole is too.
Copy-Item res/windows/LICENSE-GPL3.txt "$stage/"
Sign "$stage/openDJ.exe"
if (Test-Path "$stage/opendj-plugin-host.exe") { Sign "$stage/opendj-plugin-host.exe" }

$zip = "dist/opendj-$Version-windows-x64.zip"
if (Test-Path $zip) { Remove-Item $zip }
Compress-Archive -Path $stage -DestinationPath $zip

$iscc = "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe"
if (-not (Test-Path $iscc)) {
  choco install innosetup -y --no-progress | Out-Null
}
$source = (Resolve-Path $stage).Path
$output = (Resolve-Path dist).Path
& $iscc /Q "/DVersion=$Version" "/DSource=$source" "/DOutput=$output" res/windows/opendj.iss
if ($LASTEXITCODE) { throw "Inno Setup failed" }
Sign "dist/openDJ-$Version-windows-x64-setup.exe"
Remove-Item -Recurse -Force $stage
