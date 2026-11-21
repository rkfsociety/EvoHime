import { clipboard, Menu, shell, type BrowserWindow, type ContextMenuParams, type MenuItemConstructorOptions } from 'electron'

import type { AppLocale } from '../shared/api'
import { isAllowedExternalUrl, isProduction } from './security'

export interface ContextMenuOptions {
  readonly getLocale?: () => AppLocale
  readonly isProduction?: () => boolean
  readonly openExternal?: (url: string) => Promise<void> | void
  readonly writeClipboard?: (text: string) => void
  readonly inspectElement?: (x: number, y: number) => void
}

interface ContextMenuLabels {
  readonly undo: string
  readonly redo: string
  readonly cut: string
  readonly copy: string
  readonly paste: string
  readonly delete: string
  readonly selectAll: string
  readonly openLink: string
  readonly copyLink: string
  readonly inspect: string
}

const LABELS: Record<AppLocale, ContextMenuLabels> = {
  ru: {
    undo: 'Отменить',
    redo: 'Повторить',
    cut: 'Вырезать',
    copy: 'Копировать',
    paste: 'Вставить',
    delete: 'Удалить',
    selectAll: 'Выделить всё',
    openLink: 'Открыть ссылку в браузере',
    copyLink: 'Копировать адрес ссылки',
    inspect: 'Проверить элемент'
  },
  en: {
    undo: 'Undo',
    redo: 'Redo',
    cut: 'Cut',
    copy: 'Copy',
    paste: 'Paste',
    delete: 'Delete',
    selectAll: 'Select All',
    openLink: 'Open Link in Browser',
    copyLink: 'Copy Link Address',
    inspect: 'Inspect Element'
  }
}

/** Builds a native context menu template for editable fields, text selection, and allowed links. */
export function buildContextMenuTemplate(
  params: Pick<ContextMenuParams, 'isEditable' | 'selectionText' | 'linkURL' | 'editFlags' | 'x' | 'y'>,
  options: ContextMenuOptions = {}
): MenuItemConstructorOptions[] {
  const locale = options.getLocale?.() ?? 'ru'
  const labels = LABELS[locale]
  const prod = options.isProduction?.() ?? isProduction()
  const openExternal = options.openExternal ?? ((url: string) => shell.openExternal(url))
  const writeClipboard = options.writeClipboard ?? ((text: string) => clipboard.writeText(text))
  const items: MenuItemConstructorOptions[] = []

  if (params.isEditable) {
    items.push(
      { label: labels.undo, role: 'undo', enabled: params.editFlags.canUndo },
      { label: labels.redo, role: 'redo', enabled: params.editFlags.canRedo },
      { type: 'separator' },
      { label: labels.cut, role: 'cut', enabled: params.editFlags.canCut },
      { label: labels.copy, role: 'copy', enabled: params.editFlags.canCopy },
      { label: labels.paste, role: 'paste', enabled: params.editFlags.canPaste },
      { label: labels.delete, role: 'delete', enabled: params.editFlags.canDelete },
      { type: 'separator' },
      { label: labels.selectAll, role: 'selectAll', enabled: params.editFlags.canSelectAll }
    )
  } else if (params.selectionText.trim().length > 0) {
    items.push(
      { label: labels.copy, role: 'copy', enabled: params.editFlags.canCopy },
      { label: labels.selectAll, role: 'selectAll', enabled: params.editFlags.canSelectAll }
    )
  }

  const linkUrl = params.linkURL.trim()
  if (linkUrl.length > 0 && isAllowedExternalUrl(linkUrl)) {
    if (items.length > 0) {
      items.push({ type: 'separator' })
    }
    items.push(
      {
        label: labels.openLink,
        click: () => {
          void openExternal(linkUrl)
        }
      },
      {
        label: labels.copyLink,
        click: () => {
          writeClipboard(linkUrl)
        }
      }
    )
  }

  if (!prod && options.inspectElement) {
    const inspect = options.inspectElement
    if (items.length > 0) {
      items.push({ type: 'separator' })
    }
    items.push({
      label: labels.inspect,
      click: () => {
        inspect(params.x, params.y)
      }
    })
  }

  return items
}

/** Attaches a native right-click context menu handler to the given BrowserWindow. */
export function attachContextMenu(window: BrowserWindow, options: ContextMenuOptions = {}): void {
  window.webContents.on('context-menu', (_event, params) => {
    if (window.isDestroyed()) return
    const template = buildContextMenuTemplate(params, {
      ...options,
      inspectElement: options.inspectElement ?? ((x, y) => window.webContents.inspectElement(x, y))
    })
    if (template.length === 0) return
    Menu.buildFromTemplate(template).popup({ window })
  })
}
