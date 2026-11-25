# Gets libmpv for the Windows build: downloads the pinned build from shinchiro/mpv-winbuild-cmake,
# checks its checksum, unpacks it, and makes mpv.lib, the import library Rust's MSVC toolchain links
# against (the package only has MinGW's libmpv.dll.a).
#
#   installers/windows/get-mpv.ps1 [-Dir <folder>]          (default folder: .\mpv)
#
# Then point the build at that folder, e.g. from mpc-rv\:
#   $env:RUSTFLAGS = "-L native=$PWD\mpv"; $env:PATH = "$PWD\mpv;$env:PATH"
#   cargo test -p mpc-core; cargo run -p mpc-rv-windows -- C:\path\to\video.mkv
# The program needs libmpv-2.dll next to it, or on PATH, to start.
#
# Needs Visual Studio (or its Build Tools) with the C++ tools, for dumpbin and lib, and 7-Zip.
param([string]$Dir = "mpv")
$ErrorActionPreference = "Stop"

# The pinned build. To update: pick a release on GitHub, copy its mpv-dev-x86_64 file name and the
# sha256 shown next to it.
$Release = "20261111"
$File = "mpv-dev-x86_64-20261111-git-bcde6b93cc.7z"
$Sha256 = "273c079b11ea5575822f972756f4c9a71149a5c10130353abeb8586738938452"

New-Item -ItemType Directory -Force $Dir | Out-Null
$Dir = (Resolve-Path $Dir).Path
$archive = Join-Path $Dir $File
if (-not (Test-Path $archive)) {
    Invoke-WebRequest "https://github.com/shinchiro/mpv-winbuild-cmake/releases/download/$Release/$File" -OutFile $archive
}
$hash = (Get-FileHash $archive -Algorithm SHA256).Hash.ToLower()
if ($hash -ne $Sha256) {
    Remove-Item $archive
    throw "${File}: checksum $hash, expected $Sha256"
}
7z x -y "-o$Dir" $archive | Out-Null
if ($LASTEXITCODE) { throw "7z couldn't unpack $File" }

# mpv.lib from the DLL's exported functions, with Visual Studio's dumpbin and lib.
$vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
$vs = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if (-not $vs) { throw "Visual Studio with the C++ tools isn't installed" }
$msvc = Get-ChildItem "$vs\VC\Tools\MSVC" | Sort-Object Name | Select-Object -Last 1
$bin = Join-Path $msvc.FullName "bin\Hostx64\x64"
$dll = Join-Path $Dir "libmpv-2.dll"
# dumpbin lists each export as "ordinal hint RVA name".
$exports = & "$bin\dumpbin.exe" /nologo /exports $dll |
    Select-String '^\s+\d+\s+[0-9A-F]+\s+[0-9A-F]{8}\s+(mpv_\w+)' |
    ForEach-Object { $_.Matches[0].Groups[1].Value }
# 54 in the pinned build; far fewer means dumpbin's output wasn't read right.
if ($exports.Count -lt 40) { throw "found only $($exports.Count) mpv functions in libmpv-2.dll" }
$def = Join-Path $Dir "mpv.def"
Set-Content $def (@("LIBRARY libmpv-2", "EXPORTS") + $exports)
& "$bin\lib.exe" /nologo "/def:$def" "/out:$(Join-Path $Dir 'mpv.lib')" /machine:x64
if ($LASTEXITCODE) { throw "lib.exe couldn't make mpv.lib" }
Write-Host "libmpv $Release ready in $Dir ($($exports.Count) functions)"
