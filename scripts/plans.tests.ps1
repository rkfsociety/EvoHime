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

    # Зависимость объявляется либо на весь план ("план 01"), либо на его этап
    # ("этап 01.1"). Оба вида проверяются одинаково: номер плана обязан быть
    # меньше собственного.
    foreach ($reference in [regex]::Matches($blocking, 'план[а-я]*\s+(\d{2})(?!\.)')) {
        if ([int]$reference.Groups[1].Value -ge $number) {
            throw "$($plan.Name) blocks on plan $($reference.Groups[1].Value), which is not lower than its own number. Renumber the plans or make the dependency optional with described degradation."
        }
    }

    # Ссылка на собственный этап описывает внутренний порядок работ, а не
    # зависимость от другого плана, поэтому проверяется только чужой номер.
    foreach ($reference in [regex]::Matches($blocking, '(?<!\d)(\d{2})\.\d(?!\d)')) {
        if ([int]$reference.Groups[1].Value -gt $number) {
            throw "$($plan.Name) blocks on stage $($reference.Value), which belongs to a later plan. Renumber the plans or make the dependency optional with described degradation."
        }
    }

    # Этапы обязаны быть пронумерованы как под-планы NN.M: именно они дают
    # зависящим планам возможность не ждать план целиком.
    $stages = [regex]::Matches($text, '(?m)^### (\d{2})\.(\d) ')
    if ($stages.Count -eq 0) {
        throw "$($plan.Name) must number its stages as NN.M so other plans can depend on a stage."
    }
    $stageIndex = 1
    foreach ($stage in $stages) {
        if ([int]$stage.Groups[1].Value -ne $number) {
            throw "$($plan.Name) has a stage numbered for a different plan: $($stage.Value.Trim())."
        }
        if ([int]$stage.Groups[2].Value -ne $stageIndex) {
            throw "$($plan.Name) stage numbers must be consecutive from 1; found $($stage.Value.Trim())."
        }
        $stageIndex++
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
