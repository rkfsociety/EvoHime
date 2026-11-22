import { useId } from 'react'

export interface BrandLotusProps {
  readonly size?: number | undefined
  readonly className?: string | undefined
}

/**
 * Единый 5-лепестковый брендовый знак EvoHime.
 * Использует уникальные gradient IDs через `useId()`, чтобы не конфликтовать
 * при одновременном рендере в сайдбаре, Улье и других поверхностях.
 */
export function BrandLotus({ size = 22, className = '' }: BrandLotusProps): React.JSX.Element {
  const gradientId = useId()
  const centerId = `${gradientId}-center`
  const wingId = `${gradientId}-wing`
  const classes = className.length > 0 ? `brand-lotus ${className}` : 'brand-lotus'

  return (
    <svg className={classes} viewBox="0 0 48 48" width={size} height={size} fill="none" aria-hidden="true">
      <defs>
        <linearGradient id={centerId} x1="24" y1="7" x2="24" y2="39" gradientUnits="userSpaceOnUse">
          <stop offset="0%" stopColor="#ffffff" />
          <stop offset="55%" stopColor="#ddd6fe" />
          <stop offset="100%" stopColor="#93c5fd" />
        </linearGradient>
        <linearGradient id={wingId} x1="8" y1="16" x2="40" y2="38" gradientUnits="userSpaceOnUse">
          <stop offset="0%" stopColor="#e9d5ff" stopOpacity="0.92" />
          <stop offset="100%" stopColor="#a7f3d0" stopOpacity="0.72" />
        </linearGradient>
      </defs>
      <path d="M24 7C28.5 14 30 22 24 34C18 22 19.5 14 24 7Z" fill={`url(#${centerId})`} />
      <path d="M12 16C18.5 18 22 24.5 23.5 34.5C15.5 31.5 11.5 24.5 12 16Z" fill={`url(#${wingId})`} />
      <path d="M36 16C29.5 18 26 24.5 24.5 34.5C32.5 31.5 36.5 24.5 36 16Z" fill={`url(#${wingId})`} />
      <path d="M7 25.5C13.5 25.5 18.5 29.5 23 35.5C15 36.5 9.5 32.5 7 25.5Z" fill="#ddd6fe" fillOpacity="0.68" />
      <path d="M41 25.5C34.5 25.5 29.5 29.5 25 35.5C33 36.5 38.5 32.5 41 25.5Z" fill="#93c5fd" fillOpacity="0.68" />
    </svg>
  )
}
