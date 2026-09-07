export interface ModuleVersionRecord {
  readonly module: string
  readonly version: string
  readonly artifact: string
  readonly size: number
  readonly sha256: string
  readonly dependencies?: readonly string[]
}

export function compareSemver(left: string, right: string): number {
  const a = parseSemver(left)
  const b = parseSemver(right)
  for (let index = 0; index < 3; index += 1) {
    if (a[index] !== b[index]) return a[index]! < b[index]! ? -1 : 1
  }
  return 0
}

export function selectOutdatedModules(
  installed: Readonly<Record<string, string>>,
  available: readonly ModuleVersionRecord[]
): ModuleVersionRecord[] {
  const selected = new Map<string, ModuleVersionRecord>()
  for (const module of available) {
    const current = installed[module.module]
    if (!current || compareSemver(current, module.version) < 0) selected.set(module.module, module)
  }
  let changed = true
  while (changed) {
    changed = false
    for (const module of available) {
      if (selected.has(module.module)) continue
      if ((module.dependencies ?? []).some((dependency) => selected.has(dependency))) {
        selected.set(module.module, module)
        changed = true
      }
    }
  }
  return available.filter((module) => selected.has(module.module))
}

function parseSemver(value: string): [number, number, number] {
  const match = /^(\d+)\.(\d+)\.(\d+)$/.exec(value)
  if (!match) throw new Error(`Некорректная версия модуля: ${value}`)
  return [Number(match[1]), Number(match[2]), Number(match[3])]
}
