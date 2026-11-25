import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from 'react'

import {
  HONEYCOMB_SECTOR_CAPACITY,
  type HoneycombAgent,
  type HoneycombCity,
  type HoneycombModuleNode,
  type HoneycombSector,
  type HoneycombWorldState
} from '@shared/hive-city-map'
import type { UiIconName } from '../../UiIcon'
import {
  buildWorldRoadGraphAndPlacements,
  getCityOuterHexRadius,
  getPointyHexVertices,
  type HexCellPlacement
} from './honeycomb-geometry'
import {
  CARGO_COLORS,
  createTrafficSimulationState,
  getTrafficLightPhase,
  ROLE_COLORS,
  stepTrafficSimulation
} from './honeycomb-traffic'

export const MIN_CAMERA_ZOOM = 0.28
export const MAX_CAMERA_ZOOM = 2.6

export interface HiveCityDepartmentPin {
  readonly key: string
  readonly label: string
  readonly subtitle: string
  readonly icon: UiIconName
  readonly activeCount: number
  readonly maxCapacity: number
}

export interface HiveCityAgentHint {
  readonly id: string
  readonly displayName?: string
  readonly role: string
  readonly currentAction: string
}

export type HiveCityMapPayload =
  | { readonly available: true; readonly snapshot: import('@shared/hive-city-map').HiveCityMapSnapshot }
  | { readonly available: false; readonly snapshot: null }

export interface HiveCitySceneProps {
  readonly status: string
  readonly world?: HoneycombWorldState | undefined
  readonly queenWorking?: boolean | undefined
  readonly queenOverlay?: ReactNode | undefined
  readonly timeScale?: number | undefined
  // Совместимость с OperationsPanel.tsx до подключения живого движка (Этап 4)
  readonly departments?: readonly HiveCityDepartmentPin[] | undefined
  readonly selectedDepartment?: string | null | undefined
  readonly onSelectDepartment?: ((departmentKey: string) => void) | undefined
  readonly idleDepartmentsLabel?: string | undefined
  readonly activeDepartmentsLabel?: string | undefined
  readonly departmentIconRenderer?: ((icon: UiIconName) => ReactNode) | undefined
  readonly cityMap?: HiveCityMapPayload | null | undefined
  readonly suppressLegacyPanorama?: boolean | undefined
  readonly activeAgents?: readonly HiveCityAgentHint[] | undefined
  readonly queenOptimized?: boolean | undefined
}

interface AnimatedCellPose {
  x: number
  y: number
  scale: number
}

type SelectedTarget =
  | { readonly kind: 'citadel'; readonly city: HoneycombCity }
  | { readonly kind: 'project_hub'; readonly city: HoneycombCity }
  | { readonly kind: 'sector'; readonly city: HoneycombCity; readonly sector: HoneycombSector }
  | { readonly kind: 'module'; readonly city: HoneycombCity; readonly module: HoneycombModuleNode }

function buildFallbackWorld(status: string, queenWorking: boolean): HoneycombWorldState {
  const paused = status === 'disabled' || status === 'sleeping' || status === 'waiting_for_limits'
  return {
    schemaVersion: 2,
    paused,
    queenWorking: !paused && queenWorking,
    primaryCityId: 'city-eva',
    cities: [
      {
        id: 'city-eva',
        projectName: 'EvoHime',
        isPrimary: true,
        hasQueenCitadel: true,
        citadelExitsCount: 1,
        worldCenterX: 0,
        worldCenterY: 0,
        maxRing: 0,
        sectors: [],
        modules: [],
        edges: []
      }
    ],
    intercityLinks: [],
    activeDispatches: [],
    revision: 1
  }
}

function traceHexPath(ctx: CanvasRenderingContext2D, cx: number, cy: number, radius: number): void {
  const verts = getPointyHexVertices(cx, cy, radius)
  if (verts.length === 0) return
  ctx.beginPath()
  ctx.moveTo(verts[0]!.x, verts[0]!.y)
  for (let i = 1; i < verts.length; i += 1) {
    ctx.lineTo(verts[i]!.x, verts[i]!.y)
  }
  ctx.closePath()
}

function roleLabelRu(role: HoneycombAgent['role']): string {
  switch (role) {
    case 'executor':
      return 'Исполнитель'
    case 'analyst':
      return 'Аналитик'
    case 'strategist':
      return 'Стратег'
    case 'decision_coordinator':
      return 'Координатор решений'
  }
}

export function HiveCityScene({
  status,
  world: worldProp,
  queenWorking = false,
  queenOverlay,
  timeScale = 1
}: HiveCitySceneProps): React.JSX.Element {
  const normalizedStatus = status.trim() || 'idle'
  const world = useMemo(
    () => worldProp ?? buildFallbackWorld(normalizedStatus, queenWorking),
    [worldProp, normalizedStatus, queenWorking]
  )

  const { placements, roadGraph } = useMemo(
    () => buildWorldRoadGraphAndPlacements(world),
    [world]
  )

  const canvasRef = useRef<HTMLCanvasElement | null>(null)
  const containerRef = useRef<HTMLDivElement | null>(null)
  const cameraRef = useRef({ x: 380, y: 0, zoom: 0.45 })
  const panDragRef = useRef<{ active: boolean; lastX: number; lastY: number }>({
    active: false,
    lastX: 0,
    lastY: 0
  })
  const animatedPosesRef = useRef<Map<string, AnimatedCellPose>>(new Map())
  const trafficRef = useRef(createTrafficSimulationState())
  const worldRef = useRef(world)
  const placementsRef = useRef(placements)
  const roadGraphRef = useRef(roadGraph)
  const timeScaleRef = useRef(timeScale)

  worldRef.current = world
  placementsRef.current = placements
  roadGraphRef.current = roadGraph
  timeScaleRef.current = timeScale

  const [zoomUi, setZoomUi] = useState(0.45)
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [mmbPanning, setMmbPanning] = useState(false)

  const selectedTarget = useMemo<SelectedTarget | null>(() => {
    if (!selectedId) return null
    for (const city of world.cities) {
      if (selectedId === `${city.id}:citadel`) return { kind: 'citadel', city }
      if (selectedId === `${city.id}:hub`) return { kind: 'project_hub', city }
      const sector = city.sectors.find((s) => s.id === selectedId)
      if (sector) return { kind: 'sector', city, sector }
      const mod = city.modules.find((m) => m.id === selectedId)
      if (mod) return { kind: 'module', city, module: mod }
    }
    return null
  }, [selectedId, world.cities])

  const clampZoom = useCallback((value: number): number => {
    return Math.max(MIN_CAMERA_ZOOM, Math.min(MAX_CAMERA_ZOOM, value))
  }, [])

  const applyZoom = useCallback(
    (nextZoom: number, anchorScreenX?: number, anchorScreenY?: number): void => {
      const clamped = clampZoom(nextZoom)
      const cam = cameraRef.current
      const canvas = canvasRef.current
      if (canvas && anchorScreenX !== undefined && anchorScreenY !== undefined) {
        const rect = canvas.getBoundingClientRect()
        const cx = anchorScreenX - rect.left - rect.width / 2
        const cy = anchorScreenY - rect.top - rect.height / 2
        const worldX = cam.x + cx / cam.zoom
        const worldY = cam.y + cy / cam.zoom
        cam.zoom = clamped
        cam.x = worldX - cx / clamped
        cam.y = worldY - cy / clamped
      } else {
        cam.zoom = clamped
      }
      setZoomUi(Number(clamped.toFixed(2)))
    },
    [clampZoom]
  )

  const resetCamera = useCallback((): void => {
    cameraRef.current = { x: 380, y: 0, zoom: 0.45 }
    setZoomUi(0.45)
  }, [])

  useEffect(() => {
    const canvas = canvasRef.current
    if (!canvas) return
    const ctx = canvas.getContext('2d')
    if (!ctx) return

    let rafId = 0
    let lastTs = performance.now()
    let simTimeSec = 0

    const renderFrame = (now: number): void => {
      const dtSec = Math.min(0.05, Math.max(0.001, (now - lastTs) / 1000))
      lastTs = now
      const currentWorld = worldRef.current
      const currentPlacements = placementsRef.current
      const currentGraph = roadGraphRef.current
      const currentScale = timeScaleRef.current

      if (!currentWorld.paused) {
        simTimeSec += dtSec * Math.min(20, Math.max(1, currentScale))
      }

      // Плавная интерполяция позиций и масштаба сот (анимация роста и перестройки).
      const poses = animatedPosesRef.current
      for (const [id, placement] of currentPlacements.entries()) {
        const existing = poses.get(id)
        if (!existing) {
          poses.set(id, { x: placement.x, y: placement.y, scale: 0.05 })
        } else {
          const lerpFactor = 1 - Math.exp(-dtSec * 7.5)
          existing.x += (placement.x - existing.x) * lerpFactor
          existing.y += (placement.y - existing.y) * lerpFactor
          existing.scale += (1 - existing.scale) * lerpFactor
        }
      }
      for (const id of poses.keys()) {
        if (!currentPlacements.has(id)) {
          poses.delete(id)
        }
      }

      stepTrafficSimulation(
        trafficRef.current,
        currentWorld,
        currentGraph,
        dtSec,
        simTimeSec,
        currentScale
      )

      const dpr = window.devicePixelRatio || 1
      const width = canvas.clientWidth || 800
      const height = canvas.clientHeight || 600
      if (canvas.width !== Math.round(width * dpr) || canvas.height !== Math.round(height * dpr)) {
        canvas.width = Math.round(width * dpr)
        canvas.height = Math.round(height * dpr)
      }

      ctx.save()
      ctx.scale(dpr, dpr)

      // 1. Фон и лёгкая тактическая гекс-сетка (строгий вид сверху).
      const bgGrad = ctx.createRadialGradient(
        width / 2,
        height / 2,
        40,
        width / 2,
        height / 2,
        Math.max(width, height) * 0.75
      )
      bgGrad.addColorStop(0, '#0f172a')
      bgGrad.addColorStop(1, '#060a14')
      ctx.fillStyle = bgGrad
      ctx.fillRect(0, 0, width, height)

      const cam = cameraRef.current
      ctx.translate(width / 2, height / 2)
      ctx.scale(cam.zoom, cam.zoom)
      ctx.translate(-cam.x, -cam.y)

      // 1.5. Внешняя гексагональная сота-забор каждого города (определение границ города).
      for (const city of currentWorld.cities) {
        const maxRing = Math.max(
          city.maxRing,
          city.sectors.length > 0 ? 1 : 0,
          ...city.modules.map((m) => m.ring)
        )
        const cityHexRadius = getCityOuterHexRadius(maxRing)

        ctx.save()
        // Подложка территории города внутри забора.
        traceHexPath(ctx, city.worldCenterX, city.worldCenterY, cityHexRadius)
        ctx.fillStyle = city.hasQueenCitadel
          ? 'rgba(30, 41, 59, 0.34)'
          : 'rgba(15, 30, 52, 0.34)'
        ctx.fill()

        // Тёмное основание стены-забора.
        ctx.lineWidth = 7
        ctx.strokeStyle = '#090e1a'
        ctx.stroke()

        // Яркий внешний контур забора города.
        ctx.lineWidth = city.hasQueenCitadel ? 3.2 : 2.6
        ctx.strokeStyle = city.hasQueenCitadel ? '#f59e0b' : '#38bdf8'
        ctx.stroke()

        // Внутренняя тонкая кайма забора.
        traceHexPath(ctx, city.worldCenterX, city.worldCenterY, cityHexRadius - 7)
        ctx.lineWidth = 1.1
        ctx.strokeStyle = city.hasQueenCitadel
          ? 'rgba(251, 191, 36, 0.35)'
          : 'rgba(56, 189, 248, 0.35)'
        ctx.stroke()
        ctx.restore()
      }

      // 2. Бесшовная 3-проходная отрисовка дорожной сети:
      //    Проход A: все бордюры (второстепенные -> магистрали)
      //    Проход B: всё полотно асфальта (полностью перекрывает внутренние швы на узлах)
      //    Проход C: вся осевая разметка
      ctx.lineJoin = 'round'
      ctx.lineCap = 'round'

      const roadPasses: readonly ('curb' | 'asphalt' | 'marking')[] = ['curb', 'asphalt', 'marking']
      const roadTiers: readonly ('secondary' | 'primary')[] = ['secondary', 'primary']

      for (const pass of roadPasses) {
        for (const tier of roadTiers) {
          for (const edge of currentGraph.edges) {
            const isSecondary = edge.kind === 'driveway'
            if (tier === 'secondary' && !isSecondary) continue
            if (tier === 'primary' && isSecondary) continue

            const from = currentGraph.nodes.get(edge.fromId)
            const to = currentGraph.nodes.get(edge.toId)
            if (!from || !to) continue

            ctx.beginPath()
            ctx.moveTo(from.x, from.y)
            ctx.lineTo(to.x, to.y)

            if (pass === 'curb') {
              ctx.setLineDash([])
              ctx.strokeStyle = isSecondary ? '#273549' : '#334155'
              ctx.lineWidth = isSecondary ? 12 : edge.kind === 'intercity' ? 22 : 19
              ctx.stroke()
            } else if (pass === 'asphalt') {
              ctx.setLineDash([])
              ctx.strokeStyle = isSecondary ? '#172133' : '#0f172a'
              ctx.lineWidth = isSecondary ? 9 : edge.kind === 'intercity' ? 18 : 15
              ctx.stroke()
            } else {
              ctx.setLineDash(isSecondary ? [4, 5] : edge.kind === 'intercity' ? [10, 7] : [6, 6])
              ctx.strokeStyle =
                edge.kind === 'intercity'
                  ? 'rgba(251, 191, 36, 0.55)'
                  : isSecondary
                    ? 'rgba(148, 163, 184, 0.25)'
                    : 'rgba(226, 232, 240, 0.34)'
              ctx.lineWidth = isSecondary ? 1 : 1.3
              ctx.stroke()
            }
          }
        }
      }
      ctx.setLineDash([])

      // 3. Светофоры на перекрёстках.
      const lightPhase = getTrafficLightPhase(simTimeSec)
      const ringColor =
        lightPhase === 'ring_green' || lightPhase === 'ring_green_blink'
          ? '#22c55e'
          : lightPhase === 'amber_to_radial' || lightPhase === 'amber_to_ring'
            ? '#f59e0b'
            : '#ef4444'
      for (const node of currentGraph.nodes.values()) {
        if (node.kind !== 'intersection' || node.rayIndex === undefined) continue
        ctx.beginPath()
        ctx.arc(node.x, node.y, 4.4, 0, Math.PI * 2)
        ctx.fillStyle = '#090d16'
        ctx.fill()
        ctx.beginPath()
        ctx.arc(node.x, node.y, 2.5, 0, Math.PI * 2)
        ctx.fillStyle = ringColor
        ctx.fill()
      }

      // 4. Связи зависимостей модулей подсвечиваются только при выборе узла, чтобы не зашумлять карту.
      if (selectedId) {
        for (const city of currentWorld.cities) {
          for (const edge of city.edges) {
            if (selectedId !== edge.fromId && selectedId !== edge.toId) continue
            const fromPose = poses.get(edge.fromId)
            const toPose = poses.get(edge.toId)
            if (!fromPose || !toPose) continue
            const mx = (fromPose.x + toPose.x) / 2
            const my = (fromPose.y + toPose.y) / 2
            const vx = mx - city.worldCenterX
            const vy = my - city.worldCenterY
            const vLen = Math.max(1, Math.hypot(vx, vy))
            const cx = mx + (vx / vLen) * 38
            const cy = my + (vy / vLen) * 38
            ctx.beginPath()
            ctx.moveTo(fromPose.x, fromPose.y)
            ctx.quadraticCurveTo(cx, cy, toPose.x, toPose.y)
            ctx.strokeStyle = 'rgba(56, 189, 248, 0.88)'
            ctx.lineWidth = 2.4
            ctx.stroke()
          }
        }
      }

      // 5. Соты: Цитадель Королевы, базовые узлы, сектора (по 3 агента), модули проекта.
      for (const [id, placement] of currentPlacements.entries()) {
        const pose = poses.get(id) ?? { x: placement.x, y: placement.y, scale: 1 }
        const r = placement.radius * pose.scale
        if (r < 1) continue

        const isSelected = selectedId === id

        // Мягкая тень (имитация глубины при виде сверху 90°).
        ctx.save()
        ctx.shadowColor = 'rgba(0, 0, 0, 0.55)'
        ctx.shadowBlur = placement.kind === 'citadel' ? 24 : 12
        ctx.shadowOffsetY = 4
        traceHexPath(ctx, pose.x, pose.y, r)
        if (placement.kind === 'citadel') {
          ctx.fillStyle = '#1e1b2e'
        } else if (placement.kind === 'project_hub') {
          ctx.fillStyle = '#172554'
        } else if (placement.kind === 'sector') {
          ctx.fillStyle = '#132238'
        } else {
          ctx.fillStyle = '#162032'
        }
        ctx.fill()
        ctx.restore()

        // Внешняя фаска и контур соты (сектор окрашивается в цвет своей роли).
        const sectorRole =
          placement.kind === 'sector'
            ? currentWorld.cities
                .find((c) => c.id === placement.cityId)
                ?.sectors.find((s) => s.id === id)?.role
            : undefined
        traceHexPath(ctx, pose.x, pose.y, r)
        ctx.lineWidth = isSelected ? 2.8 : placement.kind === 'citadel' ? 2.4 : 1.7
        ctx.strokeStyle = isSelected
          ? '#fde047'
          : placement.kind === 'citadel'
            ? currentWorld.queenWorking
              ? '#fbbf24'
              : '#a16207'
            : placement.kind === 'project_hub'
              ? '#38bdf8'
              : placement.kind === 'sector'
                ? sectorRole
                  ? ROLE_COLORS[sectorRole]
                  : '#3b82f6'
                : '#475569'
        ctx.stroke()

        // Внутренняя фаска и ядро Цитадели.
        traceHexPath(ctx, pose.x, pose.y, r * 0.82)
        ctx.lineWidth = 1.1
        ctx.strokeStyle =
          placement.kind === 'citadel' ? 'rgba(251, 191, 36, 0.28)' : 'rgba(255, 255, 255, 0.09)'
        ctx.stroke()

        if (placement.kind === 'citadel') {
          traceHexPath(ctx, pose.x, pose.y, r * 0.42)
          ctx.fillStyle = currentWorld.queenWorking ? 'rgba(251, 191, 36, 0.28)' : 'rgba(161, 98, 7, 0.2)'
          ctx.fill()
          ctx.lineWidth = 1.5
          ctx.strokeStyle = '#fbbf24'
          ctx.stroke()
        }

        // Метки выходов Цитадели (перпендикулярно граням).
        if (placement.kind === 'citadel' || placement.kind === 'project_hub') {
          const apothem = r * Math.cos(Math.PI / 6)
          for (const angle of placement.exitAngles) {
            const gx = pose.x + Math.cos(angle) * apothem
            const gy = pose.y + Math.sin(angle) * apothem
            ctx.beginPath()
            ctx.arc(gx, gy, 3.8, 0, Math.PI * 2)
            ctx.fillStyle = placement.kind === 'citadel' ? '#fbbf24' : '#38bdf8'
            ctx.fill()
          }
        }

        // Отображение 3 шестиугольных гнёзд агентов внутри сектора 1-го кольца.
        if (placement.kind === 'sector') {
          const city = currentWorld.cities.find((c) => c.id === placement.cityId)
          const sector = city?.sectors.find((s) => s.id === id)
          if (sector) {
            for (let slot = 0; slot < HONEYCOMB_SECTOR_CAPACITY; slot += 1) {
              const slotAngle = -Math.PI / 2 + (slot * Math.PI * 2) / 3
              const sx = pose.x + Math.cos(slotAngle) * (r * 0.44)
              const sy = pose.y + Math.sin(slotAngle) * (r * 0.44)
              const agent = sector.agents[slot]
              traceHexPath(ctx, sx, sy, 6.2 * pose.scale)
              if (agent) {
                ctx.fillStyle = ROLE_COLORS[agent.role]
                ctx.fill()
                if (currentWorld.paused || !agent.activeTripId) {
                  ctx.strokeStyle = '#f8fafc'
                  ctx.lineWidth = 1.4
                  ctx.stroke()
                }
              } else {
                ctx.fillStyle = 'rgba(148, 163, 184, 0.16)'
                ctx.fill()
                ctx.strokeStyle = 'rgba(148, 163, 184, 0.28)'
                ctx.lineWidth = 0.8
                ctx.stroke()
              }
            }
          }
        }

        // Подпись соты в аккуратной плашке (видна при достаточном зуме).
        if (cam.zoom >= 0.42) {
          ctx.font = placement.kind === 'citadel' ? '600 11px sans-serif' : '500 10px sans-serif'
          ctx.textAlign = 'center'
          ctx.textBaseline = 'middle'
          let label = ''
          const city = currentWorld.cities.find((c) => c.id === placement.cityId)
          if (placement.kind === 'citadel') label = `Королева · ${city?.projectName ?? ''}`
          else if (placement.kind === 'project_hub') label = city?.projectName ?? 'Проект'
          else if (placement.kind === 'sector') {
            label = city?.sectors.find((s) => s.id === id)?.label ?? 'Сектор'
          } else {
            label = city?.modules.find((m) => m.id === id)?.name ?? ''
          }
          if (label) {
            const labelY = pose.y + r + 14
            const textW = ctx.measureText(label).width
            ctx.fillStyle = 'rgba(6, 10, 20, 0.78)'
            ctx.beginPath()
            ctx.roundRect(pose.x - textW / 2 - 6, labelY - 8, textW + 12, 16, 6)
            ctx.fill()
            ctx.fillStyle = placement.kind === 'citadel' ? '#fef08a' : '#e2e8f0'
            ctx.fillText(label, pose.x, labelY)
          }
        }
      }

      // 6. Машины (только при активных рейсах, при паузе на дорогах никого нет).
      for (const vehicle of trafficRef.current.vehicles.values()) {
        ctx.save()
        ctx.translate(vehicle.x, vehicle.y)
        ctx.rotate(vehicle.angleRad)

        // Корпус машины.
        ctx.fillStyle = ROLE_COLORS[vehicle.role]
        ctx.strokeStyle = '#020617'
        ctx.lineWidth = 1.2
        ctx.beginPath()
        ctx.roundRect(-7, -4, 14, 8, 2.5)
        ctx.fill()
        ctx.stroke()

        // Индикатор груза на крыше.
        ctx.fillStyle = CARGO_COLORS[vehicle.cargo]
        ctx.beginPath()
        ctx.arc(-1, 0, 2.4, 0, Math.PI * 2)
        ctx.fill()

        // Фары.
        ctx.fillStyle = vehicle.waitingAtSignal || vehicle.waitingForCarAhead ? '#ef4444' : '#fef08a'
        ctx.fillRect(5.5, -2.5, 1.5, 1.5)
        ctx.fillRect(5.5, 1, 1.5, 1.5)

        ctx.restore()
      }

      ctx.restore()
      rafId = requestAnimationFrame(renderFrame)
    }

    rafId = requestAnimationFrame(renderFrame)
    return () => cancelAnimationFrame(rafId)
  }, [selectedId])

  const handleWheel = useCallback(
    (event: React.WheelEvent<HTMLCanvasElement>): void => {
      event.preventDefault()
      const factor = event.deltaY < 0 ? 1.12 : 0.89
      applyZoom(cameraRef.current.zoom * factor, event.clientX, event.clientY)
    },
    [applyZoom]
  )

  const handlePointerDown = useCallback(
    (event: React.PointerEvent<HTMLCanvasElement>): void => {
      // СКМ (кнопка 1): панорамирование карты.
      if (event.button === 1) {
        event.preventDefault()
        panDragRef.current = { active: true, lastX: event.clientX, lastY: event.clientY }
        setMmbPanning(true)
        event.currentTarget.setPointerCapture?.(event.pointerId)
        return
      }
      // ЛКМ (кнопка 0): выбор соты по координатам.
      if (event.button === 0) {
        const canvas = canvasRef.current
        if (!canvas) return
        const rect = canvas.getBoundingClientRect()
        const cam = cameraRef.current
        const worldX = cam.x + (event.clientX - rect.left - rect.width / 2) / cam.zoom
        const worldY = cam.y + (event.clientY - rect.top - rect.height / 2) / cam.zoom

        let hit: HexCellPlacement | null = null
        let bestDist = Infinity
        for (const placement of placementsRef.current.values()) {
          const pose = animatedPosesRef.current.get(placement.id) ?? placement
          const dist = Math.hypot(worldX - pose.x, worldY - pose.y)
          if (dist <= placement.radius && dist < bestDist) {
            bestDist = dist
            hit = placement
          }
        }
        setSelectedId(hit ? hit.id : null)
      }
    },
    []
  )

  const handlePointerMove = useCallback((event: React.PointerEvent<HTMLCanvasElement>): void => {
    if (!panDragRef.current.active) return
    const dx = event.clientX - panDragRef.current.lastX
    const dy = event.clientY - panDragRef.current.lastY
    panDragRef.current.lastX = event.clientX
    panDragRef.current.lastY = event.clientY
    const cam = cameraRef.current
    cam.x -= dx / cam.zoom
    cam.y -= dy / cam.zoom
  }, [])

  const handlePointerUp = useCallback((event: React.PointerEvent<HTMLCanvasElement>): void => {
    if (event.button === 1 && panDragRef.current.active) {
      panDragRef.current.active = false
      setMmbPanning(false)
      if (event.currentTarget.hasPointerCapture?.(event.pointerId)) {
        event.currentTarget.releasePointerCapture?.(event.pointerId)
      }
    }
  }, [])

  return (
    <div
      ref={containerRef}
      className={`hive-city-scene hive-city-scene--2d${mmbPanning ? ' hive-city-scene--panning' : ''}`}
      data-testid="hive-city-scene"
      data-mode="2d-canvas"
      data-status={normalizedStatus}
    >
      <canvas
        ref={canvasRef}
        className="hive-city-scene__canvas"
        data-testid="hive-city-canvas"
        onWheel={handleWheel}
        onPointerDown={handlePointerDown}
        onPointerMove={handlePointerMove}
        onPointerUp={handlePointerUp}
        onAuxClick={(event) => {
          if (event.button === 1) event.preventDefault()
        }}
      />

      {queenOverlay ? <div className="hive-city-scene__queen-overlay">{queenOverlay}</div> : null}

      <div className="hive-city-scene__camera-hud" aria-label="Управление камерой">
        <button
          type="button"
          onClick={() => applyZoom(cameraRef.current.zoom * 1.2)}
          aria-label="Приблизить"
        >
          +
        </button>
        <span data-testid="hive-camera-zoom">{Math.round(zoomUi * 100)}%</span>
        <button
          type="button"
          onClick={() => applyZoom(cameraRef.current.zoom * 0.82)}
          aria-label="Отдалить"
        >
          −
        </button>
        <button type="button" onClick={resetCamera} aria-label="Сбросить камеру">
          Центр
        </button>
      </div>

      {selectedTarget ? (
        <aside className="hive-city-scene__inspector" data-testid="hive-cell-inspector">
          <div className="hive-city-scene__inspector-head">
            <strong>
              {selectedTarget.kind === 'citadel'
                ? `Цитадель Королевы (${selectedTarget.city.projectName})`
                : selectedTarget.kind === 'project_hub'
                  ? `Узел проекта: ${selectedTarget.city.projectName}`
                  : selectedTarget.kind === 'sector'
                    ? selectedTarget.sector.label
                    : selectedTarget.module.name}
            </strong>
            <button type="button" onClick={() => setSelectedId(null)} aria-label="Закрыть инспектор">
              ×
            </button>
          </div>
          {selectedTarget.kind === 'citadel' ? (
            <p>
              Выходов к кольцам: {selectedTarget.city.citadelExitsCount} / 6 · Секторов:{' '}
              {selectedTarget.city.sectors.length}
            </p>
          ) : null}
          {selectedTarget.kind === 'sector' ? (
            <div>
              <p>
                Мест занято: {selectedTarget.sector.agents.length} / {HONEYCOMB_SECTOR_CAPACITY}
              </p>
              <ul>
                {selectedTarget.sector.agents.map((agent) => (
                  <li key={agent.id}>
                    <span
                      className="hive-city-scene__role-dot"
                      style={{ backgroundColor: ROLE_COLORS[agent.role] }}
                    />
                    {agent.name} · {roleLabelRu(agent.role)} ({agent.topic})
                  </li>
                ))}
              </ul>
            </div>
          ) : null}
          {selectedTarget.kind === 'module' ? (
            <p>
              Кольцо {selectedTarget.module.ring} · {selectedTarget.module.relativePath} ·{' '}
              {selectedTarget.module.fileCount} файлов
            </p>
          ) : null}
        </aside>
      ) : null}
    </div>
  )
}
