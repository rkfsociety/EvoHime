import { Component, useEffect, useMemo, type ErrorInfo, type ReactNode } from 'react'
import { Canvas, useThree } from '@react-three/fiber'

import { HexPlatform } from './HexPlatform'
import { QueenSpire } from './QueenSpire'

export interface HiveCitySceneProps {
  /** Статус наблюдения Улья (`observing`, `active`, `waiting_for_limits`, `waiting_for_tasks`, `disabled`, `sleeping`). */
  readonly status: string
}

interface ErrorBoundaryState {
  readonly hasError: boolean
}

class SceneErrorBoundary extends Component<{ readonly fallback: ReactNode; readonly children: ReactNode }, ErrorBoundaryState> {
  override state: ErrorBoundaryState = { hasError: false }

  static getDerivedStateFromError(): ErrorBoundaryState {
    return { hasError: true }
  }

  override componentDidCatch(_error: Error, _info: ErrorInfo): void {
    // Изолируем сбои WebGL, чтобы 3D-сцена не ломала интерфейс Улья.
  }

  override render(): ReactNode {
    if (this.state.hasError) {
      return this.props.fallback
    }
    return this.props.children
  }
}

function isWebGLAvailable(): boolean {
  if (typeof document === 'undefined') return false
  try {
    const canvas = document.createElement('canvas')
    return Boolean(
      canvas.getContext('webgl2') ||
      canvas.getContext('webgl') ||
      canvas.getContext('experimental-webgl')
    )
  } catch {
    return false
  }
}

function resolvePlatformAccent(status: string): string {
  const normalized = status.trim().toLowerCase()
  if (normalized === 'active') return '#fbbf24'
  if (normalized === 'observing') return '#a855f7'
  return '#38bdf8'
}

function IsometricCameraRig(): null {
  const camera = useThree((state) => state.camera)
  const size = useThree((state) => state.size)

  useEffect(() => {
    camera.lookAt(0, 0.75, 0)
    const minDim = Math.min(size.width, size.height)
    if (minDim > 0 && 'zoom' in camera) {
      camera.zoom = Math.max(22, Math.min(38, minDim / 9.2))
    }
    camera.updateProjectionMatrix()
  }, [camera, size.width, size.height])

  return null
}

/**
 * Базовая изометрическая 3D-сцена «Цифрового Города» Улья.
 * Использует `OrthographicCamera`, гексагональную платформу на `InstancedMesh`
 * и центральный «Шпиль Королевы» с дыханием, синхронизированным со статусом.
 */
export function HiveCityScene({ status }: HiveCitySceneProps): React.JSX.Element {
  const webglSupported = useMemo(() => isWebGLAvailable(), [])
  const accentColor = useMemo(() => resolvePlatformAccent(status), [status])
  const normalizedStatus = status.trim().toLowerCase() || 'idle'

  const fallback = (
    <div
      className="hive-city-scene__fallback"
      data-testid="hive-city-scene-fallback"
      aria-hidden="true"
    />
  )

  return (
    <div
      className="hive-city-scene"
      data-testid="hive-city-scene"
      data-status={normalizedStatus}
      aria-hidden="true"
    >
      {webglSupported ? (
        <SceneErrorBoundary fallback={fallback}>
          <Canvas
            className="hive-city-scene__canvas"
            orthographic
            dpr={[1, 1.5]}
            camera={{ position: [12, 13, 12], zoom: 34, near: 0.1, far: 120 }}
            gl={{ alpha: true, antialias: true, powerPreference: 'high-performance' }}
            style={{ background: 'transparent', pointerEvents: 'none' }}
          >
            <IsometricCameraRig />
            <ambientLight intensity={0.55} color="#cbd5e1" />
            <directionalLight position={[10, 18, 8]} intensity={1.1} color="#e2e8f0" />
            <directionalLight position={[-10, 8, -10]} intensity={0.45} color="#8b5cf6" />

            <HexPlatform accentColor={accentColor} />
            <QueenSpire status={status} />
          </Canvas>
        </SceneErrorBoundary>
      ) : (
        fallback
      )}
    </div>
  )
}
