import { useMemo, useRef } from 'react'
import { useFrame } from '@react-three/fiber'
import * as THREE from 'three'

export interface QueenSpireProps {
  /** Статус цикла наблюдения Улья (`observing`, `active`, `waiting_for_limits`, `waiting_for_tasks`, `disabled`, `sleeping`). */
  readonly status: string
}

interface SpireVisualProfile {
  readonly primaryColor: string
  readonly secondaryColor: string
  readonly breathSpeed: number
  readonly minEmissive: number
  readonly maxEmissive: number
  readonly scaleAmplitude: number
  readonly ringSpeed: number
}

function resolveSpireProfile(status: string): SpireVisualProfile {
  const normalized = status.trim().toLowerCase()
  if (normalized === 'active') {
    return {
      primaryColor: '#fbbf24',
      secondaryColor: '#f59e0b',
      breathSpeed: 2.4,
      minEmissive: 0.7,
      maxEmissive: 1.4,
      scaleAmplitude: 0.045,
      ringSpeed: 0.55
    }
  }
  if (normalized === 'observing') {
    return {
      primaryColor: '#a855f7',
      secondaryColor: '#fbbf24',
      breathSpeed: 1.5,
      minEmissive: 0.45,
      maxEmissive: 1.0,
      scaleAmplitude: 0.03,
      ringSpeed: 0.35
    }
  }
  return {
    primaryColor: '#7c3aed',
    secondaryColor: '#6366f1',
    breathSpeed: 0.85,
    minEmissive: 0.2,
    maxEmissive: 0.48,
    scaleAmplitude: 0.018,
    ringSpeed: 0.14
  }
}

const PYLON_ANGLES = [0, Math.PI / 2, Math.PI, (3 * Math.PI) / 2]

/**
 * Центральный элемент Улья — «Шпиль Королевы» (low-poly кристалл-трон).
 * Собран из примитивов `Cylinder`, `Cone`, `Box` и орбитального кольца;
 * синхронизирует «дыхание» (`emissiveIntensity` и `scale`) со статусом демона.
 */
export function QueenSpire({ status }: QueenSpireProps): React.JSX.Element {
  const crystalGroupRef = useRef<THREE.Group>(null)
  const coreMaterialRef = useRef<THREE.MeshStandardMaterial>(null)
  const crystalMaterialRef = useRef<THREE.MeshStandardMaterial>(null)
  const ringRef = useRef<THREE.Mesh>(null)
  const lightRef = useRef<THREE.PointLight>(null)

  const profile = useMemo(() => resolveSpireProfile(status), [status])

  useFrame(({ clock }) => {
    const t = clock.getElapsedTime()
    // Плавная синусоида [0..1] для эффекта дыхания.
    const pulse = (Math.sin(t * profile.breathSpeed) + 1) * 0.5
    const emissive = profile.minEmissive + (profile.maxEmissive - profile.minEmissive) * pulse
    const scale = 1 + profile.scaleAmplitude * Math.sin(t * profile.breathSpeed)

    if (coreMaterialRef.current) {
      coreMaterialRef.current.emissiveIntensity = emissive
    }
    if (crystalMaterialRef.current) {
      crystalMaterialRef.current.emissiveIntensity = emissive * 0.9
    }
    if (crystalGroupRef.current) {
      crystalGroupRef.current.scale.set(scale, 1 + (scale - 1) * 1.4, scale)
      crystalGroupRef.current.rotation.y = t * (profile.ringSpeed * 0.45)
    }
    if (ringRef.current) {
      ringRef.current.rotation.z = -t * profile.ringSpeed
    }
    if (lightRef.current) {
      lightRef.current.intensity = 1.4 + pulse * 1.6
    }
  })

  return (
    <group name="hive-queen-spire" position={[0, 0.03, 0]}>
      {/* Нижняя шестиугольная ступень пьедестала. */}
      <mesh position={[0, 0.14, 0]}>
        <cylinderGeometry args={[1.15, 1.38, 0.28, 6]} />
        <meshStandardMaterial color="#1e293b" roughness={0.55} metalness={0.55} />
      </mesh>

      {/* Верхняя шестиугольная ступень пьедестала. */}
      <mesh position={[0, 0.36, 0]}>
        <cylinderGeometry args={[0.78, 0.96, 0.18, 6]} />
        <meshStandardMaterial color="#0f172a" roughness={0.45} metalness={0.65} />
      </mesh>

      {/* Четыре угловых пилона-контрфорса вокруг основания шпиля. */}
      {PYLON_ANGLES.map((angle, index) => {
        const radius = 0.88
        const x = Math.cos(angle) * radius
        const z = Math.sin(angle) * radius
        return (
          <mesh
            key={index}
            position={[x, 0.72, z]}
            rotation={[0, -angle, 0]}
          >
            <boxGeometry args={[0.14, 0.95, 0.2]} />
            <meshStandardMaterial
              color="#1e293b"
              emissive={profile.primaryColor}
              emissiveIntensity={0.18}
              roughness={0.4}
              metalness={0.7}
            />
          </mesh>
        )
      })}

      {/* Светящееся центральное ядро-колонна. */}
      <mesh position={[0, 1.12, 0]}>
        <cylinderGeometry args={[0.24, 0.34, 1.35, 6]} />
        <meshStandardMaterial
          ref={coreMaterialRef}
          color={profile.secondaryColor}
          emissive={profile.secondaryColor}
          emissiveIntensity={profile.minEmissive}
          roughness={0.25}
          metalness={0.4}
          toneMapped={false}
        />
      </mesh>

      {/* Дышащий верхний кристалл-шпиль Королевы. */}
      <group ref={crystalGroupRef} position={[0, 2.32, 0]}>
        <mesh position={[0, 0.62, 0]}>
          <coneGeometry args={[0.42, 1.35, 6]} />
          <meshStandardMaterial
            ref={crystalMaterialRef}
            color={profile.primaryColor}
            emissive={profile.primaryColor}
            emissiveIntensity={profile.minEmissive}
            roughness={0.2}
            metalness={0.5}
            flatShading
            toneMapped={false}
          />
        </mesh>
        <mesh position={[0, -0.32, 0]} rotation={[Math.PI, 0, 0]}>
          <coneGeometry args={[0.42, 0.52, 6]} />
          <meshStandardMaterial
            color={profile.primaryColor}
            emissive={profile.primaryColor}
            emissiveIntensity={profile.minEmissive * 0.8}
            roughness={0.25}
            metalness={0.5}
            flatShading
            toneMapped={false}
          />
        </mesh>
      </group>

      {/* Орбитальное энергетическое кольцо вокруг шпиля. */}
      <mesh ref={ringRef} position={[0, 1.65, 0]} rotation={[Math.PI / 2, 0, 0]}>
        <torusGeometry args={[0.92, 0.025, 6, 6]} />
        <meshBasicMaterial
          color={profile.primaryColor}
          transparent
          opacity={0.72}
          toneMapped={false}
        />
      </mesh>

      {/* Локальный свет от шпиля, подсвечивающий центральные гексы платформы. */}
      <pointLight
        ref={lightRef}
        position={[0, 2.5, 0]}
        color={profile.primaryColor}
        intensity={2}
        distance={9}
        decay={2}
      />
    </group>
  )
}
