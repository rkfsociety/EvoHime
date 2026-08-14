$ErrorActionPreference = 'Stop'

# Проверяет правило нумерации из docs/plans/README.md: план может блокирующе
# зависеть только от планов с меньшим номером. Именно нарушение этого правила
# однажды сделало выполнимый план похожим на заблокированный.

$plansDir = Join-Path $PSScriptRoot '..\docs\plans'
$plans = Get-ChildItem -Path $plansDir -Filter '*.md' |
    Where-Object { $_.Name -match '^(\d{2})-' } |
    Sort-Object Name

if ($plans.Count -eq 0) {
    throw 'No numbered plans found in docs/plans.'
}

$index = Join-Path $plansDir 'README.md'
if (-not (Test-Path $index)) {
    throw 'docs/plans/README.md must describe the plan order and numbering rule.'
}

# Номера обязаны идти подряд с 01: дыра означает потерянный или
# переименованный план, о котором индекс молчит.
$expected = 1
foreach ($plan in $plans) {
    $number = [int]($plan.Name -replace '^(\d{2}).*$', '$1')
    if ($number -ne $expected) {
        throw "Plan numbers must be consecutive from 01; expected $('{0:d2}' -f $expected) but found $($plan.Name)."
    }
    $expected++
}

foreach ($plan in $plans) {
    $number = [int]($plan.Name -replace '^(\d{2}).*$', '$1')
    # Планы лежат в UTF-8 без BOM: PS 5.1 иначе прочитает их как ANSI.
    $text = Get-Content -Raw -Encoding UTF8 $plan.FullName

    $dependencies = [regex]::Match($text, '(?ms)^## Зависимости.*?(?=^## |\z)')
    if (-not $dependencies.Success) {
        throw "$($plan.Name) must contain a '## Зависимости' section."
    }
    $section = $dependencies.Value

    if ($section -notmatch 'Блокирующ') {
        throw "$($plan.Name) must state its blocking dependencies explicitly."
    }

    # Блокирующая часть заканчивается там, где начинается описание
    # опциональных интеграций или того, что план предоставляет другим.
    $blockingEnd = [regex]::Match($section, 'Опциональн|Что этот план обязан|Это последний план')
    $blocking = if ($blockingEnd.Success) { $section.Substring(0, $blockingEnd.Index) } else { $section }

    foreach ($reference in [regex]::Matches($blocking, 'план[а-я]*\s+(\d{2})')) {
        $referenced = [int]$reference.Groups[1].Value
        if ($referenced -ge $number) {
            throw "$($plan.Name) blocks on plan $('{0:d2}' -f $referenced), which is not lower than its own number. Renumber the plans or make the dependency optional with described degradation."
        }
    }

    # Опциональная зависимость обязана описывать поведение до её появления:
    # без этого читатель не отличит деградацию от незавершённой работы.
    if ($section -match 'Опциональн' -and $section -notmatch 'Опциональных интеграций нет') {
        $optional = $section.Substring($section.IndexOf('Опциональн'))
        if ($optional -notmatch 'До его появления|до него|Пока плана нет|до этого|До этого') {
            throw "$($plan.Name) lists optional integrations without describing the behaviour before they exist."
        }
    }
}

Write-Output "plans order: PASS ($($plans.Count) plans)"
