export type UiIconName =
  | 'activity'
  | 'alert-triangle'
  | 'box'
  | 'chat'
  | 'check'
  | 'check-circle'
  | 'chevron-down'
  | 'chevron-right'
  | 'chevron-up'
  | 'circle'
  | 'compass'
  | 'copy'
  | 'folder'
  | 'folder-plus'
  | 'git-branch'
  | 'globe'
  | 'history'
  | 'link'
  | 'list-checks'
  | 'loader'
  | 'lock'
  | 'message'
  | 'mic'
  | 'mic-off'
  | 'panel-right'
  | 'plus'
  | 'pulse'
  | 'repeat'
  | 'send'
  | 'settings'
  | 'stop'
  | 'target'
  | 'terminal'
  | 'workflow'
  | 'x'

export interface UiIconProps {
  readonly name: UiIconName
  readonly size?: number
  readonly className?: string
}

/**
 * Единый набор линейных векторных иконок оболочки:
 * заменяет разнородные текстовые глифы Unicode на ровную пиксельную сетку.
 */
export function UiIcon({ name, size = 16, className = '' }: UiIconProps): React.JSX.Element {
  const classes = className.length > 0 ? `ui-icon ${className}` : 'ui-icon'
  const common = {
    className: classes,
    width: size,
    height: size,
    viewBox: '0 0 24 24',
    fill: 'none',
    stroke: 'currentColor',
    strokeWidth: 1.8,
    strokeLinecap: 'round' as const,
    strokeLinejoin: 'round' as const,
    'aria-hidden': true
  }

  switch (name) {
    case 'chat':
      return (
        <svg {...common}>
          <path d="M21 11.5a8.5 8.5 0 0 1-8.5 8.5c-1.4 0-2.8-.34-4-.96L3 20.5l1.46-5.12A8.46 8.46 0 0 1 4 11.5 8.5 8.5 0 0 1 12.5 3 8.5 8.5 0 0 1 21 11.5Z" />
        </svg>
      )
    case 'history':
      return (
        <svg {...common}>
          <circle cx="12" cy="12" r="8.5" />
          <path d="M12 7.5v4.8l3.2 1.9" />
        </svg>
      )
    case 'compass':
      return (
        <svg {...common}>
          <circle cx="12" cy="12" r="8.5" />
          <polygon points="15.8 8.2 13.7 13.7 8.2 15.8 10.3 10.3 15.8 8.2" />
        </svg>
      )
    case 'check-circle':
      return (
        <svg {...common}>
          <circle cx="12" cy="12" r="8.5" />
          <path d="m8.8 12.2 2.2 2.2 4.4-4.6" />
        </svg>
      )
    case 'pulse':
      return (
        <svg {...common}>
          <path d="M3 12h4l2.4-6 4.8 12L17 12h4" />
        </svg>
      )
    case 'workflow':
      return (
        <svg {...common}>
          <circle cx="6.5" cy="6.5" r="2.5" />
          <circle cx="17.5" cy="6.5" r="2.5" />
          <circle cx="12" cy="17.5" r="2.5" />
          <path d="M8.7 7.8 11 15.2M15.3 7.8 13 15.2M9 6.5h6" />
        </svg>
      )
    case 'repeat':
      return (
        <svg {...common}>
          <path d="M17 2.8 20.5 6 17 9.2" />
          <path d="M4.5 11.5V9.5A3.5 3.5 0 0 1 8 6h12" />
          <path d="M7 21.2 3.5 18 7 14.8" />
          <path d="M19.5 12.5v2A3.5 3.5 0 0 1 16 18H4" />
        </svg>
      )
    case 'terminal':
      return (
        <svg {...common}>
          <rect x="3.5" y="4.5" width="17" height="15" rx="2.5" />
          <path d="m8 10 3 2.5L8 15M12.8 15h3.4" />
        </svg>
      )
    case 'mic':
      return (
        <svg {...common}>
          <rect x="9.2" y="3.2" width="5.6" height="10.2" rx="2.8" />
          <path d="M5.8 10.8a6.2 6.2 0 0 0 12.4 0M12 17v3.8M9 20.8h6" />
        </svg>
      )
    case 'mic-off':
      return (
        <svg {...common}>
          <path d="M9.2 9.4v1.2a2.8 2.8 0 0 0 4.8 2M9.2 5.9a2.8 2.8 0 0 1 5.6.1v4.1" />
          <path d="M5.8 10.8a6.2 6.2 0 0 0 9.6 5.1M18.2 10.8c0 1.2-.35 2.33-.96 3.28M12 17v3.8M9 20.8h6M3.5 3.5l17 17" />
        </svg>
      )
    case 'list-checks':
      return (
        <svg {...common}>
          <path d="m4 6.5 1.8 1.8L9 5M4 12.5l1.8 1.8L9 11M4 18.5l1.8 1.8L9 17M12 6.5h8M12 12.5h8M12 18.5h8" />
        </svg>
      )
    case 'panel-right':
      return (
        <svg {...common}>
          <rect x="3.5" y="4.5" width="17" height="15" rx="2.5" />
          <path d="M14.5 4.5v15" />
        </svg>
      )
    case 'globe':
      return (
        <svg {...common}>
          <circle cx="12" cy="12" r="8.5" />
          <path d="M3.7 12h16.6M12 3.5c2.2 2.3 3.2 5.1 3.2 8.5s-1 6.2-3.2 8.5c-2.2-2.3-3.2-5.1-3.2-8.5s1-6.2 3.2-8.5Z" />
        </svg>
      )
    case 'activity':
      return (
        <svg {...common}>
          <rect x="4" y="4" width="16" height="16" rx="2.5" />
          <path d="M7.5 12h2.2l1.6-3.8 2.4 7.6 1.6-3.8h1.7" />
        </svg>
      )
    case 'alert-triangle':
      return (
        <svg {...common}>
          <path d="M10.4 4.3a1.8 1.8 0 0 1 3.2 0l7 12.6A1.8 1.8 0 0 1 19 19.5H5a1.8 1.8 0 0 1-1.6-2.6l7-12.6Z" />
          <path d="M12 9.5v4.2M12 16.6h.01" />
        </svg>
      )
    case 'plus':
      return (
        <svg {...common}>
          <path d="M12 5v14M5 12h14" />
        </svg>
      )
    case 'folder':
      return (
        <svg {...common}>
          <path d="M3.5 6.5h6l2 2h9v9a2 2 0 0 1-2 2h-15v-13Z" />
          <path d="M3.5 10.5h17" />
        </svg>
      )
    case 'folder-plus':
      return (
        <svg {...common}>
          <path d="M3.5 6.5h5.8l2 2H20a1.5 1.5 0 0 1 1.5 1.5v7.5a2 2 0 0 1-2 2h-14a2 2 0 0 1-2-2v-11Z" />
          <path d="M12 11.5v5M9.5 14h5" />
        </svg>
      )
    case 'message':
      return (
        <svg {...common}>
          <path d="M20 11.5a7.5 7.5 0 0 1-7.5 7.5c-1.26 0-2.46-.3-3.52-.84L4 19.5l1.34-4.54A7.46 7.46 0 0 1 5 11.5 7.5 7.5 0 0 1 12.5 4 7.5 7.5 0 0 1 20 11.5Z" />
        </svg>
      )
    case 'x':
      return (
        <svg {...common}>
          <path d="M17 7 7 17M7 7l10 10" />
        </svg>
      )
    case 'chevron-down':
      return (
        <svg {...common}>
          <path d="m6 9 6 6 6-6" />
        </svg>
      )
    case 'chevron-up':
      return (
        <svg {...common}>
          <path d="m6 15 6-6 6 6" />
        </svg>
      )
    case 'chevron-right':
      return (
        <svg {...common}>
          <path d="m9 6 6 6-6 6" />
        </svg>
      )
    case 'settings':
      return (
        <svg {...common}>
          <circle cx="12" cy="12" r="3" />
          <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1Z" />
        </svg>
      )
    case 'check':
      return (
        <svg {...common}>
          <path d="m5 12.5 4.5 4.5L19 7.5" />
        </svg>
      )
    case 'circle':
      return (
        <svg {...common}>
          <circle cx="12" cy="12" r="7.5" />
        </svg>
      )
    case 'loader':
      return (
        <svg {...common}>
          <path d="M12 3.5a8.5 8.5 0 1 1-6.01 2.49" />
        </svg>
      )
    case 'copy':
      return (
        <svg {...common}>
          <rect x="9" y="9" width="11" height="11" rx="2" />
          <path d="M5 15H4.5A1.5 1.5 0 0 1 3 13.5v-9A1.5 1.5 0 0 1 4.5 3h9A1.5 1.5 0 0 1 15 4.5V5" />
        </svg>
      )
    case 'stop':
      return (
        <svg {...common} fill="currentColor" stroke="none">
          <rect x="6.5" y="6.5" width="11" height="11" rx="2.2" />
        </svg>
      )
    case 'git-branch':
      return (
        <svg {...common}>
          <circle cx="6.5" cy="6" r="2.2" />
          <circle cx="17.5" cy="8.5" r="2.2" />
          <circle cx="6.5" cy="18" r="2.2" />
          <path d="M6.5 8.2v7.6M15.3 8.8c-4.5.4-7.2 2.4-8.8 5.2" />
        </svg>
      )
    case 'lock':
      return (
        <svg {...common}>
          <rect x="5" y="10" width="14" height="10" rx="2" />
          <path d="M8 10V7a4 4 0 0 1 8 0v3M12 14v2" />
        </svg>
      )
    case 'link':
      return (
        <svg {...common}>
          <path d="M9 17H7a5 5 0 0 1 0-10h2M15 7h2a5 5 0 0 1 0 10h-2M8 12h8" />
        </svg>
      )
    case 'box':
      return (
        <svg {...common}>
          <path d="m12 3 8 4.5v9L12 21l-8-4.5v-9L12 3Z" />
          <path d="m4.5 7.5 7.5 4 7.5-4M12 11.5V21" />
        </svg>
      )
    case 'target':
      return (
        <svg {...common}>
          <circle cx="12" cy="12" r="8.5" />
          <circle cx="12" cy="12" r="4.2" />
          <circle cx="12" cy="12" r="1.2" fill="currentColor" />
        </svg>
      )
    case 'send':
      return (
        <svg {...common}>
          <path d="m21 3-7.2 18-3.1-7.7L3 10.2 21 3Z" />
          <path d="m21 3-10.3 10.3" />
        </svg>
      )
  }
}
