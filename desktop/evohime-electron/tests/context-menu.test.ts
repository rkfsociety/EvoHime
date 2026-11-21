import { describe, expect, it, vi } from 'vitest'

const fakeElectron = vi.hoisted(() => {
  const popup = vi.fn()
  const buildFromTemplate = vi.fn(() => ({ popup }))
  const openExternal = vi.fn(() => Promise.resolve())
  const writeText = vi.fn()
  return { popup, buildFromTemplate, openExternal, writeText }
})

vi.mock('electron', () => ({
  Menu: { buildFromTemplate: fakeElectron.buildFromTemplate },
  shell: { openExternal: fakeElectron.openExternal },
  clipboard: { writeText: fakeElectron.writeText },
  app: { isPackaged: true }
}))

const { attachContextMenu, buildContextMenuTemplate } = await import('../src/main/context-menu')

const defaultEditFlags = {
  canUndo: true,
  canRedo: false,
  canCut: true,
  canCopy: true,
  canPaste: true,
  canDelete: true,
  canSelectAll: true,
  canEditRichly: false
}

describe('контекстное меню ПКМ', () => {
  it('строит пункты редактирования для input/textarea в русской локали', () => {
    const items = buildContextMenuTemplate(
      {
        isEditable: true,
        selectionText: '',
        linkURL: '',
        editFlags: defaultEditFlags,
        x: 10,
        y: 20
      },
      { getLocale: () => 'ru', isProduction: () => true }
    )

    expect(items.map((item) => item.label ?? item.type)).toEqual([
      'Отменить',
      'Повторить',
      'separator',
      'Вырезать',
      'Копировать',
      'Вставить',
      'Удалить',
      'separator',
      'Выделить всё'
    ])
    expect(items[1]?.enabled).toBe(false)
    expect(items[5]?.enabled).toBe(true)
  })

  it('строит пункты копирования и выделения для выделенного текста', () => {
    const items = buildContextMenuTemplate(
      {
        isEditable: false,
        selectionText: 'выделенный фрагмент',
        linkURL: '',
        editFlags: defaultEditFlags,
        x: 0,
        y: 0
      },
      { getLocale: () => 'en', isProduction: () => true }
    )

    expect(items.map((item) => item.label)).toEqual(['Copy', 'Select All'])
  })

  it('добавляет пункты для разрешённой внешней ссылки и игнорирует небезопасные URL', () => {
    const openExternal = vi.fn()
    const writeClipboard = vi.fn()

    const allowed = buildContextMenuTemplate(
      {
        isEditable: false,
        selectionText: '',
        linkURL: 'https://github.com/rkfsociety/EvoHime',
        editFlags: defaultEditFlags,
        x: 0,
        y: 0
      },
      { getLocale: () => 'ru', isProduction: () => true, openExternal, writeClipboard }
    )

    expect(allowed.map((item) => item.label)).toEqual([
      'Открыть ссылку в браузере',
      'Копировать адрес ссылки'
    ])
    allowed[0]?.click?.({} as never, undefined, {} as never)
    allowed[1]?.click?.({} as never, undefined, {} as never)
    expect(openExternal).toHaveBeenCalledWith('https://github.com/rkfsociety/EvoHime')
    expect(writeClipboard).toHaveBeenCalledWith('https://github.com/rkfsociety/EvoHime')

    const blocked = buildContextMenuTemplate(
      {
        isEditable: false,
        selectionText: '',
        linkURL: 'javascript:alert(1)',
        editFlags: defaultEditFlags,
        x: 0,
        y: 0
      },
      { getLocale: () => 'ru', isProduction: () => true, openExternal, writeClipboard }
    )

    expect(blocked).toEqual([])
  })

  it('добавляет инспектирование элемента только вне production', () => {
    const inspectElement = vi.fn()

    const devItems = buildContextMenuTemplate(
      {
        isEditable: false,
        selectionText: '',
        linkURL: '',
        editFlags: defaultEditFlags,
        x: 42,
        y: 84
      },
      { getLocale: () => 'ru', isProduction: () => false, inspectElement }
    )

    expect(devItems.map((item) => item.label)).toEqual(['Проверить элемент'])
    devItems[0]?.click?.({} as never, undefined, {} as never)
    expect(inspectElement).toHaveBeenCalledWith(42, 84)
  })

  it('подписывается на context-menu и открывает Menu.popup только при наличии пунктов', () => {
    fakeElectron.buildFromTemplate.mockClear()
    fakeElectron.popup.mockClear()

    let listener: ((event: unknown, params: unknown) => void) | undefined
    const fakeWindow = {
      isDestroyed: () => false,
      webContents: {
        on: (event: string, cb: (event: unknown, params: unknown) => void) => {
          if (event === 'context-menu') listener = cb
        },
        inspectElement: vi.fn()
      }
    }

    attachContextMenu(fakeWindow as never, { isProduction: () => true })
    expect(listener).toBeDefined()

    listener?.({}, {
      isEditable: false,
      selectionText: '',
      linkURL: '',
      editFlags: defaultEditFlags,
      x: 0,
      y: 0
    })
    expect(fakeElectron.popup).not.toHaveBeenCalled()

    listener?.({}, {
      isEditable: false,
      selectionText: 'текст',
      linkURL: '',
      editFlags: defaultEditFlags,
      x: 0,
      y: 0
    })
    expect(fakeElectron.buildFromTemplate).toHaveBeenCalledTimes(1)
    expect(fakeElectron.popup).toHaveBeenCalledWith({ window: fakeWindow })
  })
})
