$ErrorActionPreference = 'Stop'
$vswhere = "${env:ProgramFiles(x86)}/Microsoft Visual Studio/Installer/vswhere.exe"
$visualStudio = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if (!$visualStudio) { throw 'Visual Studio C++ build tools are required' }
& "$visualStudio/Common7/Tools/Launch-VsDevShell.ps1" -Arch amd64 -HostArch amd64
Set-Location $PSScriptRoot
$gitRoot = Split-Path (Split-Path (Get-Command git).Source)
$env:PATH = "$gitRoot/bin;$env:PATH;$gitRoot/usr/bin"
$env:RIVE_PREMAKE_ARGS = '--with_rive_text --with_rive_layout --with_rive_audio=system'
& "$gitRoot/bin/bash.exe" ../build/build_rive.sh release --toolset=msc --no-lto -- rive_native
if ($LASTEXITCODE) { throw "Native build failed: $LASTEXITCODE" }
New-Item -ItemType Directory -Force lib | Out-Null
Copy-Item out/release/rive_native.dll, out/release/rive_native.lib lib
Get-ChildItem dependencies -Directory | ForEach-Object {
    $destination = Join-Path lib/licenses $_.Name
    New-Item -ItemType Directory -Force $destination | Out-Null
    Get-ChildItem -LiteralPath $_.FullName -File |
        Where-Object Name -Match '^(LICENSE|COPYING|NOTICE|PATENTS|README)([.-]|$)' |
        Copy-Item -Destination $destination
}
cargo build --release --bins
if ($LASTEXITCODE) { throw "Screenshot build failed: $LASTEXITCODE" }
New-Item -ItemType Directory -Force bin | Out-Null
Copy-Item target/release/rive-screenshot.exe bin
Copy-Item lib/rive_native.dll target/release
cargo package --allow-dirty
if ($LASTEXITCODE) { throw "Crate packaging failed: $LASTEXITCODE" }
