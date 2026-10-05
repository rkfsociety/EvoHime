export type AppearanceTheme = 'system' | 'dark' | 'light'
export type AppearanceDensity = 'comfortable' | 'compact'
export type AppearanceAccent = 'violet' | 'blue' | 'teal' | 'rose'
export type AppearanceScale = '90' | '100' | '110'

export interface AppearanceSettings {
  readonly theme: AppearanceTheme
  readonly density: AppearanceDensity
  readonly accent: AppearanceAccent
  readonly scale: AppearanceScale
  readonly reduceMotion: boolean
}

export const DEFAULT_APPEARANCE: AppearanceSettings = {
  theme: 'system',
  density: 'comfortable',
  accent: 'violet',
  scale: '100',
  reduceMotion: false
}

const STORAGE_KEY = 'evohime.appearance.v1'
const THEMES: readonly AppearanceTheme[] = ['system', 'dark', 'light']
const DENSITIES: readonly AppearanceDensity[] = ['comfortable', 'compact']
const ACCENTS: readonly AppearanceAccent[] = ['violet', 'blue', 'teal', 'rose']
const SCALES: readonly AppearanceScale[] = ['90', '100', '110']

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

export function loadAppearance(): AppearanceSettings {
  try {
    const stored: unknown = JSON.parse(window.localStorage.getItem(STORAGE_KEY) ?? 'null')
    if (!isRecord(stored)) return DEFAULT_APPEARANCE
    return {
      theme: THEMES.includes(stored['theme'] as AppearanceTheme) ? stored['theme'] as AppearanceTheme : DEFAULT_APPEARANCE.theme,
      density: DENSITIES.includes(stored['density'] as AppearanceDensity) ? stored['density'] as AppearanceDensity : DEFAULT_APPEARANCE.density,
      accent: ACCENTS.includes(stored['accent'] as AppearanceAccent) ? stored['accent'] as AppearanceAccent : DEFAULT_APPEARANCE.accent,
      scale: SCALES.includes(stored['scale'] as AppearanceScale) ? stored['scale'] as AppearanceScale : DEFAULT_APPEARANCE.scale,
      reduceMotion: typeof stored['reduceMotion'] === 'boolean' ? stored['reduceMotion'] : DEFAULT_APPEARANCE.reduceMotion
    }
  } catch {
    return DEFAULT_APPEARANCE
  }
}

export function saveAppearance(settings: AppearanceSettings): void {
  try {
    window.localStorage.setItem(STORAGE_KEY, JSON.stringify(settings))
  } catch {
    // Storage may be unavailable; keep the in-memory setting active.
  }
}

export function applyAppearance(settings: AppearanceSettings): void {
  const root = document.documentElement
  root.dataset['theme'] = settings.theme
  root.dataset['density'] = settings.density
  root.dataset['accent'] = settings.accent
  root.dataset['reduceMotion'] = String(settings.reduceMotion)
  root.style.setProperty('--ui-scale', String(Number(settings.scale) / 100))
  root.style.setProperty('--ui-density', settings.density === 'compact' ? '0.96' : '1')
}
