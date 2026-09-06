[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$tools = Join-Path $root '.evohime-tools'
$node = Join-Path $tools 'node'
$cargoHome = Join-Path $root '.evohime-cargo'
$rustupHome = Join-Path $root '.evohime-rustup'
$temp = Join-Path $root '.evohime-temp'
$npmCache = Join-Path $root '.evohime-cargo\npm-cache'

foreach ($path in @($tools, $node, $cargoHome, $rustupHome, $temp, $npmCache)) {
    New-Item -ItemType Directory -Force -Path $path | Out-Null
}

$env:CARGO_HOME = $cargoHome
$env:RUSTUP_HOME = $rustupHome
$env:TEMP = $temp
$env:TMP = $temp
$env:npm_config_cache = $npmCache
$env:Path = "$node;$cargoHome\bin;$env:Path"

Write-Output "EvoHime project-local dependencies enabled: $root"
