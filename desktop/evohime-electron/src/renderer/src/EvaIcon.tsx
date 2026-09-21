/**
 * Единый брендовый знак EvoHime. Файл копируется Vite из канонического
 * desktop/evohime-electron/resources каталога в каждый renderer bundle.
 */
export const EVA_ICON_SRC = './evohime-agent.ico'

export function EvaIcon({ className = 'eva-icon' }: { readonly className?: string }): React.JSX.Element {
  return <img className={className} src={EVA_ICON_SRC} alt="" aria-hidden="true" draggable={false} />
}
