'use client'

import { ReactNode, useEffect } from 'react'

/* ── утилита склейки классов ─────────────────────────────────────────── */
export function cn(...parts: Array<string | false | null | undefined>): string {
  return parts.filter(Boolean).join(' ')
}

/* ── терминальная кнопка  [Метка] ────────────────────────────────────── */
export function TButton({
  children,
  onClick,
  active = false,
  disabled = false,
  danger = false,
  title,
  className,
}: {
  children: ReactNode
  onClick?: () => void
  active?: boolean
  disabled?: boolean
  danger?: boolean
  title?: string
  className?: string
}) {
  return (
    <button
      type="button"
      title={title}
      onClick={onClick}
      disabled={disabled}
      className={cn(
        'whitespace-nowrap px-0.5 outline-none transition-colors duration-75',
        'focus-visible:underline focus-visible:underline-offset-4',
        disabled && 'cursor-not-allowed text-neutral-700',
        !disabled && active && 'bg-amber-300 font-bold text-black',
        !disabled && !active && danger && 'text-red-400 hover:bg-red-400 hover:text-black',
        !disabled && !active && !danger && 'text-neutral-200 hover:bg-neutral-200 hover:text-black',
        className,
      )}
    >
      [{children}]
    </button>
  )
}

/* ── заголовок секции:  ── ТЕКСТ ──────────────────────── ────────────── */
export function Section({ title, right }: { title: string; right?: ReactNode }) {
  return (
    <div className="flex items-center gap-2 sm:gap-3">
      <span className="whitespace-nowrap text-[10px] tracking-[0.3em] text-neutral-500 sm:text-[11px]">
        {title}
      </span>
      {right ? <div className="whitespace-nowrap">{right}</div> : null}
      <span className="h-px min-w-4 flex-1 bg-neutral-800" />
    </div>
  )
}

/* ── строка параметра:  МЕТКА    содержимое ──────────────────────────── */
export function Row({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1">
      <span className="w-[6.5rem] shrink-0 text-[10px] uppercase tracking-[0.18em] text-neutral-500 sm:w-32 sm:text-[11px]">
        {label}
      </span>
      <div className="flex min-w-0 flex-wrap items-baseline gap-x-3 gap-y-1">{children}</div>
    </div>
  )
}

/* ── индикатор уровня: ▇▇▇▇▇░░░░░  -12.4 dBFS ────────────────────────── */
export function Meter({ level, cols = 26 }: { level: number; cols?: number }) {
  const n = Math.round(Math.max(0, Math.min(1, level)) * cols)
  const cells: ReactNode[] = []
  for (let i = 0; i < cols; i++) {
    const filled = i < n
    const color =
      i / cols >= 0.92 ? 'text-red-500' : i / cols >= 0.78 ? 'text-amber-300' : 'text-green-500'
    cells.push(
      <span key={i} className={filled ? color : 'text-neutral-800'}>
        {filled ? '█' : '░'}
      </span>,
    )
  }
  return <span className="tracking-tight">{cells}</span>
}

/* ── полоса прогресса  [█████░░░░░░] ─────────────────────────────────── */
export function Bar({ pos, cols = 24 }: { pos: number; cols?: number }) {
  const n = Math.round(Math.max(0, Math.min(1, pos)) * cols)
  return (
    <span className="tracking-tight text-amber-300">
      <span className="text-neutral-500">[</span>
      <span>{'█'.repeat(n)}</span>
      <span className="text-neutral-800">{'·'.repeat(cols - n)}</span>
      <span className="text-neutral-500">]</span>
    </span>
  )
}

/* ── ASCII-превью формы сигнала ──────────────────────────────────────── */
export function WaveAscii({ wave, cols = 33, rows = 5 }: { wave: 'sine' | 'saw' | 'tri'; cols?: number; rows?: number }) {
  const lines: string[] = []
  for (let r = 0; r < rows; r++) {
    let line = ''
    for (let x = 0; x < cols; x++) {
      const t = x / (cols - 1)
      let y: number
      if (wave === 'sine') y = Math.sin(t * Math.PI * 4)
      else if (wave === 'saw') y = 1 - 2 * ((t * 2) % 1)
      else y = 1 - 4 * Math.abs(((t * 2) % 1) - 0.5)
      const rr = Math.round((1 - y) / 2 * (rows - 1))
      line += rr === r ? '█' : '·'
    }
    lines.push(line)
  }
  return (
    <pre className="select-none text-[10px] leading-[1.15] text-green-500 sm:text-xs" aria-label="форма сигнала">
      {lines.join('\n')}
    </pre>
  )
}

/* ── модальное TUI-окно выбора ───────────────────────────────────────── */
export type DialogItem = { id: string; label: string; hint?: string; disabled?: boolean }

export function TDialog({
  title,
  items,
  current,
  onSelect,
  onCancel,
}: {
  title: string
  items: DialogItem[]
  current?: string | null
  onSelect: (id: string) => void
  onCancel: () => void
}) {
  useEffect(() => {
    const h = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onCancel()
    }
    window.addEventListener('keydown', h)
    return () => window.removeEventListener('keydown', h)
  }, [onCancel])

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/70 p-4"
      role="dialog"
      aria-modal="true"
      aria-label={title}
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onCancel()
      }}
    >
      <div className="w-full max-w-md border border-neutral-600 bg-black shadow-[0_0_40px_rgba(0,0,0,0.9)]">
        <div className="flex items-center gap-2 bg-neutral-300 px-2 py-1 text-black">
          <span className="truncate text-xs font-bold tracking-widest">{title}</span>
          <span className="ml-auto text-xs">✕</span>
        </div>
        <ul className="tui-scroll max-h-72 overflow-y-auto py-1 text-xs sm:text-sm" role="listbox">
          {items.map((it) => {
            const isCurrent = it.id === current
            return (
              <li key={it.id}>
                <button
                  type="button"
                  role="option"
                  aria-selected={isCurrent}
                  disabled={it.disabled}
                  onClick={() => onSelect(it.id)}
                  className={cn(
                    'flex w-full items-baseline gap-2 px-3 py-1 text-left outline-none',
                    it.disabled
                      ? 'cursor-not-allowed text-neutral-700'
                      : isCurrent
                        ? 'text-amber-300 hover:bg-neutral-200 hover:text-black focus-visible:bg-neutral-700'
                        : 'text-neutral-200 hover:bg-neutral-200 hover:text-black focus-visible:bg-neutral-700',
                  )}
                >
                  <span className="w-3 shrink-0 text-amber-300">{isCurrent ? '>' : ' '}</span>
                  <span className="truncate">{it.label}</span>
                  {it.hint ? (
                    <span className="ml-auto shrink-0 text-[10px] text-neutral-500">{it.hint}</span>
                  ) : null}
                </button>
              </li>
            )
          })}
        </ul>
        <div className="flex items-center gap-3 border-t border-neutral-800 px-3 py-1.5 text-[10px] text-neutral-500">
          <span>Enter/клик — выбрать</span>
          <span className="ml-auto">
            [<button type="button" onClick={onCancel} className="text-neutral-300 hover:bg-neutral-300 hover:text-black">Отмена</button>]
          </span>
        </div>
      </div>
    </div>
  )
}

/* ── строка состояния внизу ──────────────────────────────────────────── */
export function StatusBar({ children }: { children: ReactNode }) {
  return (
    <footer className="mt-auto shrink-0 border-t border-neutral-800 bg-black px-3 py-1.5 text-[10px] text-neutral-400 sm:px-4 sm:text-xs">
      <div className="flex flex-wrap items-baseline gap-x-4 gap-y-1">{children}</div>
    </footer>
  )
}
