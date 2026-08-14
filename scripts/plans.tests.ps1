$ErrorActionPreference = 'Stop'

# Проверяет правила из docs/plans/README.md: каждый файл — один этап, номера
# идут подряд, и этап блокирующе зависит только от более раннего этапа. Именно
# нарушение последнего правила однажды сделало выполнимый план похожим на
# заблокированный.

$plansDir = Join-Path $PSScriptRoot '..\docs\plans'
$files = Get-ChildItem -Path $plansDir -Filter '*.md' |
    Where-Object { $_.Name -match '^(\d{2})-(\d)-' } |
    Sort-Object Name

if ($files.Count -eq 0) {
    throw 'No stage files found in docs/plans.'
}

$index = Join-Path $plansDir 'README.md'
if (-not (Test-Path $index)) {
    throw 'docs/plans/README.md must describe the plan order and numbering rule.'
}

# Собрать фактическую структуру: план -> список этапов.
$structure = @{}
foreach ($file in $files) {
    $plan = [int]($file.Name -replace '^(\d{2}).*$', '$1')
    $stage = [int]($file.Name -replace '^\d{2}-(\d).*$', '$1')
    if (-not $structure.ContainsKey($plan)) { $structure[$plan] = @() }
    $structure[$plan] += $stage
}

# Номера планов идут подряд с 01: дыра означает удалённый план, о котором
# индекс молчит.
$planNumbers = $structure.Keys | Sort-Object
$expected = 1
foreach ($plan in $planNumbers) {
    if ($plan -ne $expected) {
        throw "Plan numbers must be consecutive from 01; expected $('{0:d2}' -f $expected) but found $('{0:d2}' -f $plan)."
    }
    $expected++
}

# У каждого плана есть обзор (этап 0) и хотя бы один рабочий этап, идущие подряд.
foreach ($plan in $planNumbers) {
    $stages = $structure[$plan] | Sort-Object
    if ($stages[0] -ne 0) {
        throw "Plan $('{0:d2}' -f $plan) must have an overview file numbered $('{0:d2}' -f $plan)-0-*.md."
    }
    if ($stages.Count -lt 2) {
        throw "Plan $('{0:d2}' -f $plan) must have at least one stage besides the overview."
    }
    for ($i = 0; $i -lt $stages.Count; $i++) {
        if ($stages[$i] -ne $i) {
            throw "Plan $('{0:d2}' -f $plan) stage numbers must be consecutive from 0; found $($stages[$i])."
        }
    }
}

foreach ($file in $files) {
    $plan = [int]($file.Name -replace '^(\d{2}).*$', '$1')
    $stage = [int]($file.Name -replace '^\d{2}-(\d).*$', '$1')
    # Планы лежат в UTF-8 без BOM: PS 5.1 иначе прочитает их как ANSI.
    $text = Get-Content -Raw -Encoding UTF8 $file.FullName

    $dependencies = [regex]::Match($text, '(?ms)^## Зависимости.*?(?=^## |\z)')
    if (-not $dependencies.Success) {
        throw "$($file.Name) must contain a '## Зависимости' section."
    }
    $section = $dependencies.Value

    if ($section -notmatch 'Блокирующ') {
        throw "$($file.Name) must state its blocking dependencies explicitly."
    }

    # Блокирующая часть заканчивается там, где начинается описание
    # опциональных зависимостей или того, что этап предоставляет другим.
    $blockingEnd = [regex]::Match($section, 'Опциональн|Разблокирует|Это последний|Из списка')
    $blocking = if ($blockingEnd.Success) { $section.Substring(0, $blockingEnd.Index) } else { $section }

    foreach ($reference in [regex]::Matches($blocking, '(?<!\d)(\d{2})\.(\d)(?!\d)')) {
        $referencedPlan = [int]$reference.Groups[1].Value
        $referencedStage = [int]$reference.Groups[2].Value
        if ($referencedPlan -gt $plan) {
            throw "$($file.Name) blocks on stage $($reference.Value) from a later plan. Renumber the plans or make the dependency optional with described degradation."
        }
        # Обзор (этап 0) описывает зависимости всего плана и потому вправе
        # называть любые собственные этапы.
        if ($stage -ne 0 -and $referencedPlan -eq $plan -and $referencedStage -ge $stage) {
            throw "$($file.Name) blocks on stage $($reference.Value) of its own plan, which is not earlier than itself."
        }
    }

    foreach ($reference in [regex]::Matches($blocking, 'план[а-я]*\s+(\d{2})(?!\.)')) {
        if ([int]$reference.Groups[1].Value -ge $plan) {
            throw "$($file.Name) blocks on plan $($reference.Groups[1].Value), which is not lower than its own number."
        }
    }

    # Опциональная зависимость обязана описывать поведение до её появления:
    # без этого читатель не отличит деградацию от незавершённой работы.
    if ($section -match 'Опциональн' -and $section -notmatch 'Опциональных интеграций нет') {
        $optional = $section.Substring($section.IndexOf('Опциональн'))
        if ($optional -notmatch 'До его появления|до него|до подключения|Пока плана нет|до этого|До этого|без него|Без него') {
            throw "$($file.Name) lists optional dependencies without describing the behaviour before they exist."
        }
    }
}

Write-Output "plans order: PASS ($($files.Count) files, $($planNumbers.Count) plans)"
