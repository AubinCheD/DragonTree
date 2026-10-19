#Requires -Version 5.1
<#
.SYNOPSIS
  Prépare la toolchain locale (.tools\) nécessaire pour compiler DragonTree avec la toolchain Rust GNU.

.DESCRIPTION
  La toolchain rustup `stable-x86_64-pc-windows-gnu` fournit `dlltool.exe` mais pas l'assembleur
  `as.exe`, ni toutes les bibliothèques d'import Windows. Or les crates `windows-*` récents
  (utilisés par winit, parking_lot, glutin...) génèrent leurs imports via `raw-dylib`, ce qui
  exige un dlltool complet.

  Ce script télécharge depuis repo.msys2.org (paquets officiels MSYS2, empreintes SHA-256 vérifiées) :
    - binutils : as.exe + dlltool.exe et leurs DLL d'exécution ;
    - crt-git : les bibliothèques d'import Windows absentes de la toolchain rustup.
  Rien n'est installé hors du dossier du projet. Il écrit ensuite .cargo\config.toml avec les
  chemins absolus de cette machine.

  Inutile si vous utilisez la toolchain MSVC (Visual Studio Build Tools + « Développement C++ »).

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File scripts\setup-toolchain.ps1
#>
param(
    [switch]$Force
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$root  = Split-Path -Parent $PSScriptRoot
$tools = Join-Path $root '.tools'
$dl    = Join-Path $tools 'dl'
$bin   = Join-Path $tools 'mingw64\bin'
$lib   = Join-Path $tools 'mingw64\lib'
$tar   = Join-Path $env:SystemRoot 'System32\tar.exe'
$base  = 'https://repo.msys2.org/mingw/mingw64/'

$packages = [ordered]@{
    'mingw-w64-x86_64-binutils-2.47-4-any.pkg.tar.zst'                       = 'e36c52a7fe1d104c2efd97dc74162386eabf8323f6aeb9b83aeb574b291a7da0'
    'mingw-w64-x86_64-libwinpthread-14.0.0.r426.g4564ee4b5-2-any.pkg.tar.zst' = 'ac791deaf730ef7aa6fa66f31ce6ec53a02c43fa85562233a981c84c93456760'
    'mingw-w64-x86_64-zlib-1.3.2-2-any.pkg.tar.zst'                          = '9e75842a070ba648e986e12424e1c92c9d7d77200e85f6a34eeb600819f2e694'
    'mingw-w64-x86_64-zstd-1.5.7-2-any.pkg.tar.zst'                          = '1add6705b344664f6aca108c85f79ab5bdd9e1162662bb06a4cf40a34f6e0907'
    'mingw-w64-x86_64-gettext-runtime-1.0-1-any.pkg.tar.zst'                 = 'be68d7f260633284b910c588c6d82ee304a81c8817a686d2cd9df83f872c27af'
    'mingw-w64-x86_64-libiconv-1.19-1-any.pkg.tar.zst'                       = '21e334d0911f25de75d3e18e0697648bcecfa9658256d600cad0827d719c2f35'
    'mingw-w64-x86_64-crt-git-13.0.0.r380.gb83511db8-1-any.pkg.tar.zst'      = 'f2d7563f8684de1b8b65d22e6667570c1adc6a2fa63b91cdc7a94a2f0797ea35'
}

if (-not (Test-Path $tar)) { throw "tar.exe introuvable ($tar) : Windows 10 1803+ requis." }

# .NET directement : fonctionne même quand Windows PowerShell 5.1 hérite des modules de PowerShell 7.
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
function Get-Sha256([string]$path) {
    $sha = [System.Security.Cryptography.SHA256]::Create()
    $stream = [System.IO.File]::OpenRead($path)
    try { return ([System.BitConverter]::ToString($sha.ComputeHash($stream)) -replace '-', '').ToLowerInvariant() }
    finally { $stream.Dispose(); $sha.Dispose() }
}

$sysroot = (& rustc --print sysroot).Trim()
$selfContained = Join-Path $sysroot 'lib\rustlib\x86_64-pc-windows-gnu\lib\self-contained'
if (-not (Test-Path $selfContained)) {
    throw "Toolchain rustup x86_64-pc-windows-gnu introuvable (attendu : $selfContained)."
}

if ($Force -and (Test-Path $tools)) { Remove-Item -Recurse -Force $tools }
New-Item -ItemType Directory -Force -Path $dl, $bin, $lib | Out-Null

foreach ($name in $packages.Keys) {
    $file = Join-Path $dl $name
    if (-not (Test-Path $file)) {
        Write-Host "Téléchargement de $name"
        (New-Object System.Net.WebClient).DownloadFile($base + $name, $file)
    }
    $hash = Get-Sha256 $file
    if ($hash -ne $packages[$name]) {
        Remove-Item -Force $file
        throw "Empreinte SHA-256 inattendue pour $name ($hash). Fichier supprimé."
    }
}

$extract = Join-Path $tools 'extract'
if (Test-Path $extract) { Remove-Item -Recurse -Force $extract }
New-Item -ItemType Directory -Force -Path $extract | Out-Null

foreach ($name in $packages.Keys) {
    & $tar -xf (Join-Path $dl $name) -C $extract
    if ($LASTEXITCODE -ne 0) { throw "Échec d'extraction de $name" }
}

# Outils : as.exe + dlltool.exe + DLL d'exécution (cherchées dans le dossier de l'exécutable).
Copy-Item -Force (Join-Path $extract 'mingw64\bin\as.exe') $bin
Copy-Item -Force (Join-Path $extract 'mingw64\bin\dlltool.exe') $bin
Get-ChildItem (Join-Path $extract 'mingw64\bin') -Filter '*.dll' | Copy-Item -Destination $bin -Force

# Bibliothèques d'import : seulement celles qui manquent à rustup, jamais celles du CRT
# (elles doivent rester celles de la toolchain pour correspondre à crt2.o).
$crtPattern = '^lib(msvcr|ucrt|mingw|moldname|gcc|pthread|m\.a$|crtdll|msvcp|vcruntime|api-ms-win|winpthread|dloadhelper|portabledll|setargv)'
$copied = 0
Get-ChildItem (Join-Path $extract 'mingw64\lib') -Filter 'lib*.a' | ForEach-Object {
    if ($_.Name -match $crtPattern) { return }
    if (Test-Path (Join-Path $selfContained $_.Name)) { return }
    Copy-Item -Force $_.FullName $lib
    $copied++
}
Remove-Item -Recurse -Force $extract

$dlltool = (Join-Path $bin 'dlltool.exe') -replace '\\', '/'
$libDir  = $lib -replace '\\', '/'
$config  = @"
# Généré par scripts/setup-toolchain.ps1 — chemins propres à cette machine.
# Permet de compiler avec la toolchain Rust GNU sans MinGW installé (voir le script).
[target.x86_64-pc-windows-gnu]
rustflags = [
  "-C", "dlltool=$dlltool",
  "-L", "native=$libDir",
]
"@
$cargoDir = Join-Path $root '.cargo'
New-Item -ItemType Directory -Force -Path $cargoDir | Out-Null
[System.IO.File]::WriteAllText((Join-Path $cargoDir 'config.toml'), $config, (New-Object System.Text.UTF8Encoding($false)))

& (Join-Path $bin 'as.exe') --version | Select-Object -First 1
Write-Host "OK : $copied bibliothèques d'import installées dans $lib"
Write-Host "OK : .cargo\config.toml écrit. Vous pouvez lancer : cargo build --release"
