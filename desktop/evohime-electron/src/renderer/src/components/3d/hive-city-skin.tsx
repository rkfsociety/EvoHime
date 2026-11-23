import {
  createContext,
  useContext,
  useEffect,
  useMemo,
  type ReactNode
} from 'react'
import * as THREE from 'three'
import {
  CAPITAL_TOWER_INNER_RADIUS,
  displayIslandHue,
  fnv1a32,
  type CityBuildingKind,
  type CityIslandView
} from '@shared/hive-city-map'

/** Reusable per-frame scratch objects: no 60 Hz allocations inside useFrame. */
export const FRAME_DUMMY = new THREE.Object3D()

/**
 * Visual height of a tower: low-weight buildings stay readable instead of
 * collapsing into flat plates (display-only; the data keeps `height_weight_bp`).
 */
export function towerDisplayHeight(heightWeight: number): number {
  return Math.max(1.1, heightWeight * 4.0)
}

/** Цвет категории модуля для цоколя и световых поясов здания. */
export function buildingKindColor(kind: CityBuildingKind): string {
  switch (kind) {
    case 'citadel':
      return '#f0c45a'
    case 'gateway':
      return '#f0b849'
    case 'storage':
      return '#4ec9a4'
    case 'ui':
      return '#a78bfa'
    case 'worker':
      return '#38bdf8'
    case 'service':
      return '#5ea8ff'
    case 'library':
      return '#818cf8'
    case 'module':
    default:
      return '#6d92e8'
  }
}

/** Читаемая подпись типа модуля для 3D-маркера. */
export function buildingKindLabel(kind: CityBuildingKind): string {
  switch (kind) {
    case 'citadel':
      return 'Цитадель'
    case 'gateway':
      return 'Шлюз'
    case 'storage':
      return 'Хранилище'
    case 'ui':
      return 'Интерфейс'
    case 'worker':
      return 'Воркер'
    case 'service':
      return 'Сервис'
    case 'library':
      return 'Библиотека'
    case 'module':
    default:
      return 'Модуль'
  }
}

const FACADE_VARIANTS = 4

export interface FacadeSet {
  readonly material: THREE.MeshStandardMaterial
  readonly albedo: THREE.CanvasTexture
  readonly emissive: THREE.CanvasTexture
}

function createFacadeSet(variant: number): FacadeSet {
  const width = 128
  const height = 256
  const columns = 6
  const rows = 14
  const canvas = document.createElement('canvas')
  canvas.width = width
  canvas.height = height
  const context = canvas.getContext('2d')
  const emissiveCanvas = document.createElement('canvas')
  emissiveCanvas.width = width
  emissiveCanvas.height = height
  const emissiveContext = emissiveCanvas.getContext('2d')
  if (context !== null && emissiveContext !== null) {
    const bases = ['#2d385e', '#283254', '#333f68', '#2a3558']
    context.fillStyle = bases[variant % bases.length] ?? '#2d385e'
    context.fillRect(0, 0, width, height)
    emissiveContext.fillStyle = '#000000'
    emissiveContext.fillRect(0, 0, width, height)
    const litColors = ['#ffd37a', '#ffb861', '#ffe8b8', '#8cc4ff']
    const cellWidth = width / columns
    const cellHeight = height / rows
    for (let row = 0; row < rows; row += 1) {
      for (let column = 0; column < columns; column += 1) {
        const seed = fnv1a32(`facade-${variant}-${row}-${column}`)
        if (seed % 100 < 48) {
          const color = litColors[(seed >>> 7) % litColors.length] ?? '#ffd37a'
          context.fillStyle = color
          emissiveContext.fillStyle = color
        } else {
          context.fillStyle = '#141b33'
          emissiveContext.fillStyle = '#000000'
        }
        context.fillRect(
          column * cellWidth + cellWidth * 0.24,
          row * cellHeight + cellHeight * 0.26,
          cellWidth * 0.52,
          cellHeight * 0.46
        )
        emissiveContext.fillRect(
          column * cellWidth + cellWidth * 0.24,
          row * cellHeight + cellHeight * 0.26,
          cellWidth * 0.52,
          cellHeight * 0.46
        )
      }
    }
  }
  const albedo = new THREE.CanvasTexture(canvas)
  albedo.colorSpace = THREE.SRGBColorSpace
  const emissive = new THREE.CanvasTexture(emissiveCanvas)
  emissive.colorSpace = THREE.SRGBColorSpace
  const material = new THREE.MeshStandardMaterial({
    map: albedo,
    emissive: new THREE.Color('#ffffff'),
    emissiveMap: emissive,
    emissiveIntensity: 1.15,
    roughness: 0.62,
    metalness: 0.24
  })
  return { material, albedo, emissive }
}

/** Shared per-canvas materials of the whole archipelago. */
export interface CitySkin {
  readonly facades: readonly FacadeSet[]
  readonly roof: THREE.MeshStandardMaterial
  readonly plinth: THREE.MeshStandardMaterial
  readonly gold: THREE.MeshStandardMaterial
  readonly roadBed: THREE.MeshBasicMaterial
  readonly drivewayBed: THREE.MeshBasicMaterial
  readonly roadCurb: THREE.MeshBasicMaterial
}

function createCitySkin(): CitySkin {
  return {
    facades: Array.from({ length: FACADE_VARIANTS }, (_, index) => createFacadeSet(index)),
    roof: new THREE.MeshStandardMaterial({ color: '#1b2340', roughness: 0.75, metalness: 0.18 }),
    plinth: new THREE.MeshStandardMaterial({ color: '#202948', roughness: 0.82, metalness: 0.14 }),
    gold: new THREE.MeshStandardMaterial({
      color: new THREE.Color('#f0cf7b'),
      emissive: new THREE.Color('#e8b34a'),
      emissiveIntensity: 0.55,
      roughness: 0.28,
      metalness: 0.65
    }),
    roadBed: new THREE.MeshBasicMaterial({
      color: '#0b101e',
      transparent: true,
      opacity: 0.94
    }),
    drivewayBed: new THREE.MeshBasicMaterial({
      color: '#1f2a46',
      transparent: true,
      opacity: 0.94
    }),
    roadCurb: new THREE.MeshBasicMaterial({
      color: '#6b91dc',
      transparent: true,
      opacity: 0.75,
      depthWrite: false
    })
  }
}

const CitySkinContext = createContext<CitySkin | null>(null)

export function CitySkinProvider({ children }: { children: ReactNode }): React.JSX.Element {
  const skin = useMemo(() => createCitySkin(), [])
  useEffect(() => () => {
    for (const facade of skin.facades) {
      facade.albedo.dispose()
      facade.emissive.dispose()
      facade.material.dispose()
    }
    skin.roof.dispose()
    skin.plinth.dispose()
    skin.gold.dispose()
    skin.roadBed.dispose()
    skin.drivewayBed.dispose()
    skin.roadCurb.dispose()
  }, [skin])
  return <CitySkinContext.Provider value={skin}>{children}</CitySkinContext.Provider>
}

export function useCitySkin(): CitySkin {
  const contextSkin = useContext(CitySkinContext)
  return useMemo(() => contextSkin ?? createCitySkin(), [contextSkin])
}

/** Акцентный свет материалов острова: тонкие световые пояса зданий и кромка плиты. */
export function useIslandAccent(hue: number): THREE.MeshStandardMaterial {
  const material = useMemo(
    () =>
      new THREE.MeshStandardMaterial({
        color: new THREE.Color().setHSL(hue / 360, 0.62, 0.54),
        emissive: new THREE.Color().setHSL(hue / 360, 0.78, 0.52),
        emissiveIntensity: 0.8,
        roughness: 0.35,
        metalness: 0.3
      }),
    [hue]
  )
  useEffect(() => () => material.dispose(), [material])
  return material
}

/** Верхняя плита острова: благородный ночной сланец с чёткой дорожной разметкой. */
export function createStreetsTexture(view: CityIslandView): THREE.CanvasTexture {
  const size = 512
  const canvas = document.createElement('canvas')
  canvas.width = size
  canvas.height = size
  const context = canvas.getContext('2d')
  const hue = displayIslandHue(view.island.id, view.island.role)

  if (context !== null) {
    const centre = size / 2
    const gradient = context.createRadialGradient(centre, centre, 20, centre, centre, size / 2)
    gradient.addColorStop(0, '#222f58')
    gradient.addColorStop(0.5, '#17203e')
    gradient.addColorStop(1, '#10162b')
    context.fillStyle = gradient
    context.fillRect(0, 0, size, size)

    context.strokeStyle = 'rgba(130, 165, 255, 0.07)'
    context.lineWidth = 1
    const step = size / 16
    for (let pos = step; pos < size; pos += step) {
      context.beginPath()
      context.moveTo(pos, 0)
      context.lineTo(pos, size)
      context.stroke()
      context.beginPath()
      context.moveTo(0, pos)
      context.lineTo(size, pos)
      context.stroke()
    }

    if (view.island.role === 'capital') {
      const ringRadius = (CAPITAL_TOWER_INNER_RADIUS / view.radius) * centre
      const plazaGradient = context.createRadialGradient(centre, centre, 8, centre, centre, ringRadius * 0.58)
      plazaGradient.addColorStop(0, 'rgba(240, 200, 108, 0.24)')
      plazaGradient.addColorStop(1, 'rgba(240, 200, 108, 0)')
      context.fillStyle = plazaGradient
      context.beginPath()
      context.arc(centre, centre, ringRadius * 0.58, 0, Math.PI * 2)
      context.fill()

      context.strokeStyle = 'rgba(120, 168, 255, 0.2)'
      context.lineWidth = 3
      context.beginPath()
      context.arc(centre, centre, ringRadius * 1.18, 0, Math.PI * 2)
      context.stroke()

      context.strokeStyle = 'rgba(240, 204, 114, 0.28)'
      context.lineWidth = 4
      context.beginPath()
      context.arc(centre, centre, ringRadius * 0.76, 0, Math.PI * 2)
      context.stroke()
    } else {
      const beaconGrad = context.createRadialGradient(centre, centre, 4, centre, centre, size * 0.22)
      beaconGrad.addColorStop(0, `hsla(${hue}, 80%, 66%, 0.26)`)
      beaconGrad.addColorStop(1, `hsla(${hue}, 80%, 66%, 0)`)
      context.fillStyle = beaconGrad
      context.beginPath()
      context.arc(centre, centre, size * 0.22, 0, Math.PI * 2)
      context.fill()

      context.strokeStyle = `hsla(${hue}, 75%, 65%, 0.18)`
      context.lineWidth = 2.5
      context.beginPath()
      context.arc(centre, centre, size * 0.28, 0, Math.PI * 2)
      context.stroke()
    }
  }
  const texture = new THREE.CanvasTexture(canvas)
  texture.colorSpace = THREE.SRGBColorSpace
  return texture
}

/**
 * Компактная 3D-табличка в пространстве сцены (CanvasTexture sprite).
 * Используется для названий островов, секторов департаментов и инспектируемых зданий.
 */
export function LabelSprite3D({
  text,
  subtitle,
  accentColor,
  position,
  scale,
  fontSize = 24
}: {
  text: string
  subtitle?: string | undefined
  accentColor: string
  position: readonly [number, number, number]
  scale: readonly [number, number, number]
  fontSize?: number
}) {
  const texture = useMemo(() => {
    const canvas = document.createElement('canvas')
    canvas.width = 320
    canvas.height = subtitle ? 84 : 64
    const context = canvas.getContext('2d')
    if (context) {
      const radius = 14
      context.fillStyle = 'rgba(11, 15, 30, 0.88)'
      if (typeof context.roundRect === 'function') {
        context.beginPath()
        context.roundRect(3, 3, canvas.width - 6, canvas.height - 6, radius)
        context.fill()
        context.strokeStyle = accentColor
        context.lineWidth = 2.5
        context.stroke()
      } else {
        context.fillRect(2, 2, canvas.width - 4, canvas.height - 4)
        context.strokeStyle = accentColor
        context.lineWidth = 2.5
        context.strokeRect(3, 3, canvas.width - 6, canvas.height - 6)
      }

      context.fillStyle = '#eef3ff'
      context.textAlign = 'center'
      context.textBaseline = 'middle'
      let px = fontSize
      context.font = `600 ${px}px system-ui, -apple-system, sans-serif`
      const maxWidth = canvas.width - 36
      while (px > 13 && context.measureText(text).width > maxWidth) {
        px -= 2
        context.font = `600 ${px}px system-ui, -apple-system, sans-serif`
      }
      const titleY = subtitle ? canvas.height * 0.36 : canvas.height * 0.52
      context.fillText(text, canvas.width / 2, titleY, maxWidth)

      if (subtitle) {
        context.fillStyle = '#9bb0e0'
        context.font = '500 16px system-ui, -apple-system, sans-serif'
        context.fillText(subtitle, canvas.width / 2, canvas.height * 0.72, maxWidth)
      }
    }
    const created = new THREE.CanvasTexture(canvas)
    created.colorSpace = THREE.SRGBColorSpace
    return created
  }, [text, subtitle, accentColor, fontSize])

  useEffect(() => () => texture.dispose(), [texture])

  return (
    <sprite position={[position[0], position[1], position[2]]} scale={[scale[0], scale[1], scale[2]]}>
      <spriteMaterial map={texture} transparent depthWrite={false} />
    </sprite>
  )
}

/**
 * Плоская надпись на поверхности земли/сектора (лежит в плоскости XZ,
 * ориентирована вдоль сектора и никогда не перекрывает дома и дороги сверху).
 */
export function GroundLabel3D({
  text,
  subtitle,
  accentColor,
  position,
  size,
  rotationZ = 0,
  fontSize = 26
}: {
  text: string
  subtitle?: string | undefined
  accentColor: string
  position: readonly [number, number, number]
  size: readonly [number, number]
  rotationZ?: number
  fontSize?: number
}) {
  const texture = useMemo(() => {
    const canvas = document.createElement('canvas')
    canvas.width = 512
    canvas.height = subtitle ? 128 : 96
    const context = canvas.getContext('2d')
    if (context) {
      const radius = 18
      context.fillStyle = 'rgba(10, 14, 28, 0.78)'
      if (typeof context.roundRect === 'function') {
        context.beginPath()
        context.roundRect(4, 4, canvas.width - 8, canvas.height - 8, radius)
        context.fill()
        context.strokeStyle = accentColor
        context.lineWidth = 3
        context.stroke()
      } else {
        context.fillRect(4, 4, canvas.width - 8, canvas.height - 8)
      }

      context.fillStyle = '#f2f6ff'
      context.textAlign = 'center'
      context.textBaseline = 'middle'
      let px = fontSize
      context.font = `700 ${px}px system-ui, -apple-system, sans-serif`
      const maxWidth = canvas.width - 40
      while (px > 16 && context.measureText(text).width > maxWidth) {
        px -= 2
        context.font = `700 ${px}px system-ui, -apple-system, sans-serif`
      }
      const titleY = subtitle ? canvas.height * 0.36 : canvas.height * 0.52
      context.fillText(text, canvas.width / 2, titleY, maxWidth)

      if (subtitle) {
        context.fillStyle = '#a7bce8'
        context.font = '500 22px system-ui, -apple-system, sans-serif'
        context.fillText(subtitle, canvas.width / 2, canvas.height * 0.73, maxWidth)
      }
    }
    const created = new THREE.CanvasTexture(canvas)
    created.colorSpace = THREE.SRGBColorSpace
    return created
  }, [text, subtitle, accentColor, fontSize])

  useEffect(() => () => texture.dispose(), [texture])

  return (
    <mesh position={[position[0], position[1], position[2]]} rotation={[-Math.PI / 2, 0, rotationZ]}>
      <planeGeometry args={[size[0], size[1]]} />
      <meshBasicMaterial map={texture} transparent depthWrite={false} />
    </mesh>
  )
}
