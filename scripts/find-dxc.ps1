# Prints the folder of the Windows SDK's DirectX shader compiler
# (dxcompiler.dll, with dxil.dll beside it when the SDK has one; x64),
# the redistributable copy first, then the newest SDK's.
#
# wgpu compiles shaders for DirectX 12 with DXC when it finds dxcompiler.dll
# (next to openDJ.exe, or on PATH) and with the old FXC otherwise. FXC
# miscompiles the waveform shader (no beat grid, wrong colours) and takes
# minutes on some of its paths, so the Windows build ships DXC and the
# Windows tests run with it.
$ErrorActionPreference = "Stop"
$kits = "${env:ProgramFiles(x86)}\Windows Kits\10"
$dirs = @("$kits\Redist\D3D\x64")
$dirs += Get-ChildItem "$kits\bin" -Directory -ErrorAction SilentlyContinue |
  Sort-Object Name -Descending | ForEach-Object { Join-Path $_.FullName "x64" }
foreach ($dir in $dirs) {
  if (Test-Path "$dir\dxcompiler.dll") {
    return $dir
  }
}
throw "No dxcompiler.dll in the Windows SDK ($kits)"
