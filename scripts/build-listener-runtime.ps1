[CmdletBinding()]
param(
    [string]$OutputPath = (Join-Path $PSScriptRoot '..\artifacts\listener-runtime'),
    [string]$WorkPath = (Join-Path $PSScriptRoot '..\artifacts\listener-runtime-work'),
    [ValidateSet('small', 'base', 'tiny')]
    [string[]]$Rungs = @('small', 'base', 'tiny'),
    [switch]$SkipBuild
)

<#
    .SYNOPSIS
    Собирает поставку движка распознавания: whisper.dll, её зависимости,
    модели лестницы и манифест `listener-runtime.json`.

    .DESCRIPTION
    Результат этого скрипта — содержимое релиза с тегом `listener-runtime`,
    из которого Electron (`desktop/evohime-electron/src/main/update/listener-runtime.ts`)
    скачивает набор, а листенер (`crates/evohime-listener/src/tools_dir.rs`)
    его проверяет.

    Три вещи здесь принципиальны.

    1. Версия whisper.cpp закреплена коммитом, а не тегом: тег в чужом
       репозитории можно передвинуть, коммит — нет. Зеркало
       `whisper_full_params` в `engine/whisper_dll.rs` верно ровно для этой
       раскладки.
    2. Раскладка сверяется на сборке. Пробник компилируется против тех же
       заголовков, что и DLL, и его `sizeof` сравнивается с зеркалом. Иначе
       расхождение дошло бы до пользователя как `abi_unsupported` — после
       загрузки почти гигабайта.
    3. Модели проверяются по SHA-1, опубликованному апстримом для
       закреплённого коммита. Это единственный хеш, который whisper.cpp
       публикует; в наш манифест уезжает уже SHA-256, посчитанный здесь.

    CMake нужен только этому скрипту. Сборка самого продукта его не требует и
    требовать не должна: self-update ставит Git, Node, Rustup и MSVC Build
    Tools, и добавление CMake сломало бы обновление из исходников. Поэтому DLL
    приезжает готовой, а не собирается у пользователя.
#>

$ErrorActionPreference = 'Stop'

if ($PSVersionTable.PSVersion.Major -lt 7) {
    throw 'Нужен PowerShell 7 или новее. Run: pwsh -File .\scripts\build-listener-runtime.ps1'
}

. (Join-Path $PSScriptRoot 'native-package.ps1')

# ---------------------------------------------------------------------------
# Закреплённый апстрим
# ---------------------------------------------------------------------------

# Тег — для человека, коммит — для проверки. Менять их можно только вместе со
# сверкой зеркала `whisper_full_params`: несовпадение раскладки остановит
# сборку на шаге проверки ABI, а не молча уедет в релиз.
$whisperRepository = 'https://github.com/ggml-org/whisper.cpp.git'
$whisperTag = 'v1.9.3'
$whisperCommit = '371b5a7561823ab2bb32142d2751e35e7534727b'

# Ревизия поставки при неизменном апстриме: пересборка с другим набором
# ступеней или другими флагами обязана менять версию, иначе клиент не увидит
# разницы между старым и новым набором.
$runtimeRevision = 1

# SHA-1 моделей из `models/README.md` закреплённого коммита whisper.cpp.
# Апстрим публикует только SHA-1, поэтому он и служит здесь корнем доверия к
# скачанному файлу; SHA-256 для нашего манифеста считается ниже по факту.
$modelSources = [ordered]@{
    small = [pscustomobject]@{ File = 'ggml-small.bin'; Sha1 = '55356645c2b361a969dfd0ef2c5a50d530afd8d5' }
    base  = [pscustomobject]@{ File = 'ggml-base.bin'; Sha1 = '465707469ff3a37a2b9b8d8f89f2f99de7299dac' }
    tiny  = [pscustomobject]@{ File = 'ggml-tiny.bin'; Sha1 = 'bd577a113a864445d4c299885e0cb97d4ba92b5f' }
}
$modelBaseUrl = 'https://huggingface.co/ggerganov/whisper.cpp/resolve/main'

# Лестница идёт от тяжёлой ступени к лёгкой; манифест сохраняет этот порядок.
$ladder = @('small', 'base', 'tiny')

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path

function Resolve-AgainstRepo {
    param([Parameter(Mandatory)][string]$Path)

    $candidate = if ([System.IO.Path]::IsPathRooted($Path)) { $Path } else { Join-Path $repoRoot $Path }
    return [System.IO.Path]::GetFullPath($candidate)
}

$resolvedOutput = Resolve-AgainstRepo -Path $OutputPath
$resolvedWork = Resolve-AgainstRepo -Path $WorkPath
$sourcePath = Join-Path $resolvedWork 'whisper.cpp'
$buildPath = Join-Path $resolvedWork 'build'
$probePath = Join-Path $resolvedWork 'abi-probe'
$modelCachePath = Join-Path $resolvedWork 'models'

# ---------------------------------------------------------------------------
# Вспомогательные функции
# ---------------------------------------------------------------------------

function Assert-Tool {
    param([Parameter(Mandatory)][string]$Name, [Parameter(Mandatory)][string]$Hint)

    if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) {
        throw "Не найден $Name. $Hint"
    }
}

function Get-PinnedSource {
    <#
        .SYNOPSIS
        Готовит checkout whisper.cpp ровно на закреплённом коммите.
    #>
    if (Test-Path -LiteralPath (Join-Path $sourcePath '.git')) {
        Push-Location $sourcePath
        try {
            $head = (& git rev-parse HEAD 2>$null)
            if ($LASTEXITCODE -eq 0 -and $head -eq $whisperCommit) {
                Write-Host "whisper.cpp уже на $whisperTag ($whisperCommit)."
                return
            }
        }
        finally { Pop-Location }
        Remove-Item -LiteralPath $sourcePath -Recurse -Force
    }

    New-Item -ItemType Directory -Path $sourcePath -Force | Out-Null
    Push-Location $sourcePath
    try {
        Invoke-NativeCommand -Executable 'git' -Arguments @('init', '--quiet')
        Invoke-NativeCommand -Executable 'git' -Arguments @('remote', 'add', 'origin', $whisperRepository)
        # Забираем один коммит по его хешу: тег в этой цепочке не участвует,
        # поэтому передвинутый тег ничего не меняет.
        Invoke-NativeCommand -Executable 'git' -Arguments @('fetch', '--depth', '1', '--quiet', 'origin', $whisperCommit)
        Invoke-NativeCommand -Executable 'git' -Arguments @('checkout', '--quiet', 'FETCH_HEAD')
        $head = (& git rev-parse HEAD).Trim()
        if ($head -ne $whisperCommit) {
            throw "whisper.cpp: получен коммит $head вместо закреплённого $whisperCommit."
        }
    }
    finally { Pop-Location }
    Write-Host "whisper.cpp получен на $whisperTag ($whisperCommit)."
}

function Build-WhisperLibrary {
    <#
        .SYNOPSIS
        Собирает whisper.cpp как набор DLL для распространения.

        .DESCRIPTION
        `GGML_NATIVE=OFF` обязателен: с нативной оптимизацией DLL собралась бы
        под инструкции агента сборки и падала бы на чужой машине. Статический
        CRT убирает зависимость от VC++ Redistributable — её у пользователя
        может не быть, а Rust-часть продукта и так линкует CRT статически.
    #>
    $arguments = @(
        '-S', $sourcePath,
        '-B', $buildPath,
        '-A', 'x64',
        '-DBUILD_SHARED_LIBS=ON',
        '-DCMAKE_MSVC_RUNTIME_LIBRARY=MultiThreaded',
        '-DCMAKE_POLICY_DEFAULT_CMP0091=NEW',
        '-DGGML_NATIVE=OFF',
        '-DGGML_OPENMP=OFF',
        '-DWHISPER_BUILD_TESTS=OFF',
        '-DWHISPER_BUILD_EXAMPLES=OFF',
        '-DWHISPER_BUILD_SERVER=OFF'
    )
    Invoke-NativeCommand -Executable 'cmake' -Arguments $arguments
    Invoke-NativeCommand -Executable 'cmake' -Arguments @('--build', $buildPath, '--config', 'Release', '--parallel')
}

function Get-BuiltLibraries {
    <#
        .SYNOPSIS
        Собирает список DLL поставки.

        .DESCRIPTION
        Состав ggml-библиотек апстрим меняет между версиями, поэтому список не
        захардкожен: берутся все DLL сборки. Роль `support_dll` обязательна,
        потому что загрузчик Windows подтянет эти файлы сам — их хеш должен
        быть сверен до загрузки whisper.dll, а не после.
    #>
    $binaries = Get-ChildItem -LiteralPath $buildPath -Recurse -Filter '*.dll' |
        Where-Object { $_.FullName -match '\\Release\\' }
    if (-not $binaries) { throw "Сборка не дала ни одной DLL в $buildPath." }

    $whisper = @($binaries | Where-Object { $_.Name -ieq 'whisper.dll' })
    if ($whisper.Count -ne 1) {
        throw "Ожидалась ровно одна whisper.dll, найдено $($whisper.Count)."
    }

    $result = [System.Collections.Generic.List[object]]::new()
    $result.Add([pscustomobject]@{ Role = 'whisper_dll'; Path = $whisper[0].FullName })
    foreach ($item in $binaries | Where-Object { $_.Name -ine 'whisper.dll' } | Sort-Object Name) {
        $result.Add([pscustomobject]@{ Role = 'support_dll'; Path = $item.FullName })
    }
    return $result
}

function Assert-NoRedistributableDependency {
    <#
        .SYNOPSIS
        Проверяет, что DLL не требует VC++ Redistributable.

        .DESCRIPTION
        Имена импортируемых библиотек лежат в PE открытым ASCII, поэтому
        поиск по байтам находит их без dumpbin, которого на агенте может не
        быть. Ложное срабатывание останавливает сборку — это безопасная
        сторона: пропущенная зависимость превращается в `load_failed` на
        чужой машине, где отладить её уже нельзя.
    #>
    param([Parameter(Mandatory)][string]$Path)

    $forbidden = @('VCRUNTIME140', 'MSVCP140', 'VCOMP140')
    $bytes = [System.IO.File]::ReadAllBytes($Path)
    $text = [System.Text.Encoding]::ASCII.GetString($bytes)
    foreach ($name in $forbidden) {
        if ($text.Contains($name)) {
            throw "$([System.IO.Path]::GetFileName($Path)) импортирует $name — нужен VC++ Redistributable, которого у пользователя может не быть."
        }
    }
}

function Get-DeclaredAbi {
    <#
        .SYNOPSIS
        Считает размеры структур whisper.cpp по заголовкам собранного апстрима.

        .DESCRIPTION
        Пробник не линкуется с whisper: `sizeof` нужен только заголовок.
        Компилируется тем же тулчейном, что и DLL, поэтому раскладка получается
        та же самая, а не «похожая».
    #>
    Remove-Item -LiteralPath $probePath -Recurse -Force -ErrorAction SilentlyContinue
    New-Item -ItemType Directory -Path $probePath -Force | Out-Null

    $probeSource = @'
#include <stdio.h>
#include "whisper.h"

int main(void) {
    printf("%zu %zu\n", sizeof(struct whisper_context_params), sizeof(struct whisper_full_params));
    return 0;
}
'@
    Set-Content -LiteralPath (Join-Path $probePath 'main.c') -Value $probeSource -Encoding utf8NoBOM

    $probeProject = @'
cmake_minimum_required(VERSION 3.20)
project(evohime-abi-probe C)
add_executable(abi-probe main.c)
target_include_directories(abi-probe PRIVATE ${WHISPER_INCLUDE} ${GGML_INCLUDE})
'@
    Set-Content -LiteralPath (Join-Path $probePath 'CMakeLists.txt') -Value $probeProject -Encoding utf8NoBOM

    $probeBuild = Join-Path $probePath 'build'
    # Вывод cmake уходит на консоль, а не в поток функции: иначе он смешался бы
    # с результатом, и разбор `sizeof` получил бы строки лога.
    Invoke-NativeCommand -Executable 'cmake' -Arguments @(
        '-S', $probePath,
        '-B', $probeBuild,
        '-A', 'x64',
        "-DWHISPER_INCLUDE=$(Join-Path $sourcePath 'include')",
        "-DGGML_INCLUDE=$(Join-Path $sourcePath 'ggml\include')"
    ) | Out-Host
    Invoke-NativeCommand -Executable 'cmake' -Arguments @('--build', $probeBuild, '--config', 'Release') | Out-Host

    $probeExe = Get-ChildItem -LiteralPath $probeBuild -Recurse -Filter 'abi-probe.exe' | Select-Object -First 1
    if (-not $probeExe) { throw 'Пробник ABI не собрался.' }
    $output = (& $probeExe.FullName)
    if ($LASTEXITCODE -ne 0) { throw 'Пробник ABI завершился с ошибкой.' }

    $parts = ($output | Select-Object -First 1).Trim() -split '\s+'
    if ($parts.Count -ne 2) { throw "Пробник ABI напечатал непонятное: $output" }
    return [pscustomobject]@{
        ContextParamsSize = [uint32]$parts[0]
        FullParamsSize    = [uint32]$parts[1]
    }
}

function Get-MirroredAbi {
    <#
        .SYNOPSIS
        Читает раскладку, которую зеркалит листенер.
    #>
    Push-Location $repoRoot
    try {
        $output = (& cargo run --quiet --locked -p evohime-listener --example print-abi)
        if ($LASTEXITCODE -ne 0) { throw 'Не удалось прочитать раскладку ABI из листенера.' }
    }
    finally { Pop-Location }
    return ($output | Select-Object -First 1) | ConvertFrom-Json
}

function Get-Model {
    <#
        .SYNOPSIS
        Скачивает модель ступени и сверяет её с закреплённым SHA-1.
    #>
    param([Parameter(Mandatory)][string]$Rung)

    $source = $modelSources[$Rung]
    New-Item -ItemType Directory -Path $modelCachePath -Force | Out-Null
    $target = Join-Path $modelCachePath $source.File

    if (Test-Path -LiteralPath $target) {
        $cached = (Get-FileHash -LiteralPath $target -Algorithm SHA1).Hash.ToLowerInvariant()
        if ($cached -eq $source.Sha1) {
            Write-Host "Модель $Rung уже скачана."
            return $target
        }
        Remove-Item -LiteralPath $target -Force
    }

    Write-Host "Скачивание модели $Rung ($($source.File))…"
    Invoke-NativeCommand -Executable 'curl.exe' -Arguments @(
        '--location', '--fail', '--silent', '--show-error',
        '--retry', '3', '--retry-delay', '5',
        '--output', $target,
        "$modelBaseUrl/$($source.File)"
    ) | Out-Host

    $actual = (Get-FileHash -LiteralPath $target -Algorithm SHA1).Hash.ToLowerInvariant()
    if ($actual -ne $source.Sha1) {
        Remove-Item -LiteralPath $target -Force
        throw "Модель $Rung не совпала с закреплённым SHA-1: получен $actual, ожидался $($source.Sha1)."
    }
    return $target
}

function New-ManifestEntry {
    param(
        [Parameter(Mandatory)][string]$KindField,
        [Parameter(Mandatory)][string]$Kind,
        [Parameter(Mandatory)][string]$Path
    )

    $item = Get-Item -LiteralPath $Path
    if ($item.Length -le 0) { throw "Пустой файл поставки: $Path" }
    return [ordered]@{
        $KindField = $Kind
        name       = $item.Name
        sha256     = (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
        size       = $item.Length
    }
}

# ---------------------------------------------------------------------------
# Сборка
# ---------------------------------------------------------------------------

Assert-Tool -Name 'git' -Hint 'Нужен Git для получения исходников whisper.cpp.'
Assert-Tool -Name 'cmake' -Hint 'Нужен CMake 3.20+ и MSVC Build Tools для сборки whisper.dll.'
Assert-Tool -Name 'cargo' -Hint 'Нужен Rust MSVC toolchain для сверки раскладки ABI.'
Assert-Tool -Name 'curl.exe' -Hint 'Нужен curl для загрузки моделей.'

New-Item -ItemType Directory -Path $resolvedWork -Force | Out-Null

Get-PinnedSource
if (-not $SkipBuild) {
    Build-WhisperLibrary
}

$libraries = Get-BuiltLibraries
foreach ($library in $libraries) {
    Assert-NoRedistributableDependency -Path $library.Path
}

$declared = Get-DeclaredAbi
$mirrored = Get-MirroredAbi
if ([uint32]$mirrored.context_params_size -ne $declared.ContextParamsSize -or
    [uint32]$mirrored.full_params_size -ne $declared.FullParamsSize) {
    throw @"
Раскладка whisper.cpp $whisperTag не совпадает с зеркалом листенера.
  whisper.h: context_params=$($declared.ContextParamsSize), full_params=$($declared.FullParamsSize)
  зеркало:   context_params=$($mirrored.context_params_size), full_params=$($mirrored.full_params_size)
Приведите `WhisperFullParams` в crates/evohime-listener/src/engine/whisper_dll.rs к заголовку
закреплённого коммита либо закрепите другой коммит whisper.cpp.
"@
}
Write-Host "ABI сверен: context_params=$($declared.ContextParamsSize), full_params=$($declared.FullParamsSize)."

# Каталог собирается заново: остаток прошлой сборки — это лишняя DLL, а лишнюю
# DLL рядом с манифестом листенер считает поводом не запускаться вовсе.
if (Test-Path -LiteralPath $resolvedOutput) {
    Remove-Item -LiteralPath $resolvedOutput -Recurse -Force
}
New-Item -ItemType Directory -Path $resolvedOutput -Force | Out-Null

$files = [System.Collections.Generic.List[object]]::new()
foreach ($library in $libraries) {
    $target = Join-Path $resolvedOutput ([System.IO.Path]::GetFileName($library.Path))
    Copy-Item -LiteralPath $library.Path -Destination $target -Force
    $files.Add((New-ManifestEntry -KindField 'role' -Kind $library.Role -Path $target))
}

$models = [System.Collections.Generic.List[object]]::new()
foreach ($rung in $ladder) {
    if ($Rungs -notcontains $rung) { continue }
    $downloaded = Get-Model -Rung $rung
    $target = Join-Path $resolvedOutput ([System.IO.Path]::GetFileName($downloaded))
    Copy-Item -LiteralPath $downloaded -Destination $target -Force
    $models.Add((New-ManifestEntry -KindField 'rung' -Kind $rung -Path $target))
}
if ($models.Count -eq 0) { throw 'Поставка без единой ступени лестницы бесполезна.' }

$version = "whisper-$whisperTag-r$runtimeRevision"
$manifest = [ordered]@{
    schema  = 1
    version = $version
    abi     = [ordered]@{
        name                = $mirrored.name
        context_params_size = $declared.ContextParamsSize
        full_params_size    = $declared.FullParamsSize
    }
    files   = @($files.ToArray())
    models  = @($models.ToArray())
}

$manifestPath = Join-Path $resolvedOutput 'listener-runtime.json'
$json = $manifest | ConvertTo-Json -Depth 6
Set-Content -LiteralPath $manifestPath -Value $json -Encoding utf8NoBOM

# Потолок манифеста проверяет и листенер, и Electron. Упереться в него сборкой
# проще, чем получить отказ у пользователя.
$manifestSize = (Get-Item -LiteralPath $manifestPath).Length
if ($manifestSize -gt 64KB) { throw "Манифест поставки превысил 64 КБ: $manifestSize байт." }

# Записи манифеста — упорядоченные словари, а не объекты, поэтому сумма
# считается по ключу, а не через Measure-Object -Property.
$entries = @($files.ToArray()) + @($models.ToArray())
$totalBytes = 0L
foreach ($entry in $entries) { $totalBytes += [int64]$entry['size'] }

# Последняя проверка — производственным кодом листенера, а не своим повтором
# той же логики: расхождение между манифестом и каталогом должно всплыть
# здесь, а не у пользователя в виде отказа движка.
Push-Location $repoRoot
try {
    Invoke-NativeCommand -Executable 'cargo' -Arguments @(
        'run', '--quiet', '--locked', '-p', 'evohime-listener',
        '--example', 'verify-runtime', '--', $resolvedOutput
    ) | Out-Host
}
finally { Pop-Location }
Write-Host ''
Write-Host "Поставка $version собрана в $resolvedOutput"
Write-Host "  файлов: $($files.Count), ступеней: $($models.Count), суммарно $([math]::Round($totalBytes / 1MB)) МБ"
foreach ($entry in $entries) {
    Write-Host "  $($entry['name']) — $($entry['size']) байт"
}
