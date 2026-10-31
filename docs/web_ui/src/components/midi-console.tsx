'use client'

import { useEffect, useMemo, useRef, useState } from 'react'
import {
  Bar,
  cn,
  Meter,
  Row,
  Section,
  StatusBar,
  TButton,
  TDialog,
  WaveAscii,
  type DialogItem,
} from './tui'

/* ═══════════════════════ константы и справочники ═══════════════════════ */

const NOTE_NAMES = ['C', 'C#', 'D', 'D#', 'E', 'F', 'F#', 'G', 'G#', 'A', 'A#', 'B']
const noteName = (n: number) => `${NOTE_NAMES[((n % 12) + 12) % 12]}${Math.floor(n / 12) - 1}`
const fmtTime = (s: number) =>
  `${String(Math.floor(s / 60)).padStart(2, '0')}:${String(Math.floor(s % 60)).padStart(2, '0')}`
const fmtBytes = (b: number) =>
  b < 1024 * 1024 ? `${Math.round(b / 1024)} КБ` : `${(b / 1048576).toFixed(1)} МБ`
const dbfs = (lvl: number) => (lvl <= 0.0005 ? '-\u221E' : (20 * Math.log10(lvl)).toFixed(1))

const WAVE_RU = { sine: 'Синусоида', saw: 'Пила', tri: 'Треугольник' } as const
type Wave = keyof typeof WAVE_RU

const ADSR_PRESETS = {
  Пианино: { a: 0.003, d: 0.5, s: 0.35, r: 0.35 },
  Клависин: { a: 0.002, d: 0.28, s: 0.2, r: 0.18 },
  Орган: { a: 0.03, d: 0.06, s: 1.0, r: 0.09 },
  Флейта: { a: 0.12, d: 0.09, s: 0.85, r: 0.22 },
} as const
type AdsrName = keyof typeof ADSR_PRESETS
type Adsr = { a: number; d: number; s: number; r: number }

const MIDI_DEVICES = [
  { id: 'through', label: 'Midi Through:Midi Through Port-0 14:0' },
  { id: 'mpk', label: 'AKAI MPK mini 3 — USB MIDI' },
  { id: 'keystep', label: 'Arturia KeyStep 37 — USB MIDI' },
  { id: 'busy', label: 'TinyUSB MIDI 1.0', hint: 'занято' },
  { id: 'none', label: '— без устройства —' },
]

const MIDI_FILES = [
  { id: 'bach', label: 'bach_bwv_846_prelude.mid', dur: 84 },
  { id: 'gym', label: 'gymnopedie_no1_satie.mid', dur: 168 },
  { id: 'mozart', label: 'mozart_k545_allegro.mid', dur: 132 },
  { id: 'turkish', label: 'turkish_march_rondo.mid', dur: 96 },
]

const SF2_FILES = [
  { id: 'fluid', label: 'FluidR3_GM.SF2', hint: '141 МБ' },
  { id: 'gu', label: 'GeneralUser GS v1.471.sf2', hint: '30 МБ' },
  { id: 'arachno', label: 'Arachno SoundFont 1.0.sf2', hint: '130 МБ' },
  { id: 'broken', label: 'broken_font.sf2', hint: 'повреждён' },
  { id: 'missing', label: 'missing_font.sf2', hint: 'нет на диске' },
  { id: 'none', label: '— не выбирать —' },
]

const GM_PRESETS = [
  '000:000 Yamaha Grand Piano',
  '000:016 Hammond Organ',
  '000:019 Church Organ',
  '000:040 Violin',
  '000:048 Strings Ensemble',
  '000:056 Trumpet',
]

const AUDIO_DEVICES = [
  { id: 'pipewire', label: 'PipeWire Sound Server', params: '48000 Hz / 256 кадров · 2 канала' },
  { id: 'alsa', label: 'ALSA: HDA Intel PCH', params: '44100 Hz / 512 кадров · стерео' },
  { id: 'jack', label: 'JACK Audio Server', params: '48000 Hz / 128 кадров · стерео' },
  { id: 'none', label: '— без устройства —' },
]

const FOLDERS = ['/home/user/Музыка', '/home/user/Записи', '/home/roman/downloads', '/mnt/usb-audio']

const FORMATS = ['FLAC', 'OGG', 'MIDI'] as const
type RecFormat = (typeof FORMATS)[number]

/* раскладка компьютера → ноты (две октавы) */
const KEY_MAP: Record<string, number> = {
  KeyZ: 48, KeyS: 49, KeyX: 50, KeyD: 51, KeyC: 52, KeyV: 53, KeyG: 54, KeyB: 55,
  KeyH: 56, KeyN: 57, KeyJ: 58, KeyM: 59, Comma: 60, KeyL: 61, Period: 62, Semicolon: 63, Slash: 64,
  KeyQ: 60, Digit2: 61, KeyW: 62, Digit3: 63, KeyE: 64, KeyR: 65, Digit5: 66, KeyT: 67,
  Digit6: 68, KeyY: 69, Digit7: 70, KeyU: 71, KeyI: 72, Digit9: 73, KeyO: 74, Digit0: 75, KeyP: 76,
}

/* секвенция для демо-воспроизведения MIDI-файла (Am–F–C–G) */
const PATTERN = [
  57, 64, 69, 72, 76, 72, 69, 64,
  53, 60, 65, 69, 72, 69, 65, 60,
  48, 55, 60, 64, 67, 64, 60, 55,
  55, 62, 67, 71, 74, 71, 67, 62,
]
const STEP = 0.25

type DialogKind = 'midi-dev' | 'midi-file' | 'sf2' | 'preset' | 'audio-out' | 'folder'
type Held = { note: number; until: number; vel: number }

/* общий неизменяемый пустой набор нот (для производных значений) */
const EMPTY_NOTES: ReadonlySet<number> = new Set()

/* ═══════════════════════ пианино (источник «Клавиатура») ══════════════ */

function Piano({
  pressed,
  onDown,
  onUp,
}: {
  pressed: Set<number>
  onDown: (n: number) => void
  onUp: (n: number) => void
}) {
  const whites = [48, 50, 52, 53, 55, 57, 59, 60, 62, 64, 65, 67, 69, 71, 72]
  const blacks = useMemo(() => {
    const arr: { note: number; left: number }[] = []
    whites.forEach((n, i) => {
      if ([0, 2, 5, 7, 9].includes(n % 12) && i + 1 < whites.length) {
        arr.push({ note: n + 1, left: ((i + 1) * 100) / whites.length })
      }
    })
    return arr
  }, [whites])

  return (
    <div className="relative flex h-16 w-full select-none sm:h-20" aria-label="экранная клавиатура">
      {whites.map((n) => {
        const on = pressed.has(n)
        return (
          <button
            key={n}
            type="button"
            aria-label={`нота ${noteName(n)}`}
            aria-pressed={on}
            onPointerDown={(e) => {
              e.preventDefault()
              e.currentTarget.setPointerCapture(e.pointerId)
              onDown(n)
            }}
            onPointerUp={() => onUp(n)}
            onPointerCancel={() => onUp(n)}
            className={cn(
              'relative flex-1 rounded-b-sm border border-r-0 last:border-r',
              'transition-colors duration-75 touch-none',
              on ? 'z-[5] border-amber-300 bg-amber-300' : 'border-neutral-700 bg-neutral-950',
            )}
          >
            <span
              className={cn(
                'pointer-events-none absolute bottom-0.5 left-1/2 -translate-x-1/2 text-[8px] tabular-nums',
                on ? 'text-black' : 'text-neutral-600 hidden sm:inline',
              )}
            >
              {noteName(n)}
            </span>
          </button>
        )
      })}
      {blacks.map(({ note, left }) => {
        const on = pressed.has(note)
        return (
          <button
            key={note}
            type="button"
            aria-label={`нота ${noteName(note)}`}
            aria-pressed={on}
            onPointerDown={(e) => {
              e.preventDefault()
              e.currentTarget.setPointerCapture(e.pointerId)
              onDown(note)
            }}
            onPointerUp={() => onUp(note)}
            onPointerCancel={() => onUp(note)}
            style={{ left: `${left}%`, width: `${(100 / whites.length) * 0.58}%` }}
            className={cn(
              'absolute top-0 z-10 h-[62%] -translate-x-1/2 rounded-b-sm border touch-none',
              'transition-colors duration-75',
              on ? 'border-amber-300 bg-amber-300' : 'border-neutral-600 bg-neutral-900',
            )}
          >
            {on && (
              <span className="pointer-events-none absolute bottom-0.5 left-1/2 -translate-x-1/2 text-[8px] tabular-nums text-black">
                {noteName(note)}
              </span>
            )}
          </button>
        )
      })}
    </div>
  )
}

/* ═══════════════════════════ главная консоль ══════════════════════════ */

export default function MidiConsole() {
  /* ── источники ── */
  const [pressedKbd, setPressedKbd] = useState<Set<number>>(new Set())
  const [pressedMidi, setPressedMidi] = useState<Set<number>>(new Set())
  const [pressedFile, setPressedFile] = useState<Set<number>>(new Set())
  const [midiDevId, setMidiDevId] = useState<string | null>('through')
  const [midiErr, setMidiErr] = useState<string | null>(null)
  const [fileId, setFileId] = useState<string | null>('bach')
  const [fileState, setFileState] = useState<'stopped' | 'playing' | 'paused'>('stopped')
  const [filePos, setFilePos] = useState(0)

  /* ── обработчик ── */
  const [engine, setEngine] = useState<'synth' | 'sf2'>('sf2')
  const [wave, setWave] = useState<Wave>('sine')
  const [adsr, setAdsr] = useState<Adsr>({ ...ADSR_PRESETS['Пианино'] })
  const [adsrPreset, setAdsrPreset] = useState<AdsrName | 'Своё'>('Пианино')
  const [sf2Id, setSf2Id] = useState<string | null>('fluid')
  const [sf2Err, setSf2Err] = useState<string | null>(null)
  const [preset, setPreset] = useState(GM_PRESETS[0])

  /* ── выход ── */
  const [audioDevId, setAudioDevId] = useState<string | null>('pipewire')
  const [master, setMaster] = useState(100)
  const [meterL, setMeterL] = useState(0)
  const [meterR, setMeterR] = useState(0)
  const [cpu, setCpu] = useState(3)
  const [recFolder, setRecFolder] = useState(FOLDERS[0])
  const [recFormat, setRecFormat] = useState<RecFormat>('FLAC')
  const [recording, setRecording] = useState(false)
  const [recFile, setRecFile] = useState('')
  const [recTime, setRecTime] = useState(0)
  const [recSize, setRecSize] = useState(0)
  const [recSaved, setRecSaved] = useState<{ name: string; size: number } | null>(null)

  const [lastEvent, setLastEvent] = useState<string | null>(null)
  const [dialog, setDialog] = useState<DialogKind | null>(null)

  /* ── зеркала для интервалов/обработчиков ── */
  const kbdRef = useRef(pressedKbd)
  const dialogRef = useRef(dialog)
  const playRef = useRef(fileState)
  const fileIdRef = useRef(fileId)
  const durRef = useRef(MIDI_FILES[0].dur)
  const audioRef = useRef(audioDevId)
  const masterRef = useRef(master)
  const recRef = useRef({ recording, format: recFormat as RecFormat, time: 0, size: 0 })
  const simRef = useRef<Held[]>([])
  const simSigRef = useRef('')
  const fileNotesRef = useRef<Held[]>([])
  const fileSigRef = useRef('')
  const posRef = useRef(0)
  const stepRef = useRef(0)
  const accRef = useRef(0)
  const togglePlayRef = useRef<() => void>(() => {})

  /* зеркала для интервалов/обработчиков — синхронизируются после рендера */
  useEffect(() => {
    kbdRef.current = pressedKbd
    dialogRef.current = dialog
    playRef.current = fileState
    fileIdRef.current = fileId
    durRef.current = MIDI_FILES.find((f) => f.id === fileId)?.dur ?? 0
    audioRef.current = audioDevId
    masterRef.current = master
    recRef.current.recording = recording
    recRef.current.format = recFormat
  })

  /* ── производные значения ── */
  const midiDevName = MIDI_DEVICES.find((d) => d.id === midiDevId)?.label ?? null
  const sf2Sel = SF2_FILES.find((f) => f.id === sf2Id)
  const sf2Ok = !!sf2Sel && sf2Id !== 'none' && !sf2Err
  const audioSel = AUDIO_DEVICES.find((d) => d.id === audioDevId)
  const audioOk = !!audioSel && audioDevId !== 'none'
  const fileSel = MIDI_FILES.find((f) => f.id === fileId)
  const fileDur = fileSel?.dur ?? 0
  const midiActive = !!midiDevId && midiDevId !== 'none' && !midiErr
  const pressedMidiView = midiActive ? pressedMidi : EMPTY_NOTES
  const voices = Math.min(64, pressedKbd.size + pressedMidiView.size + pressedFile.size)

  /* ── ноты пианино (клавиатура-источник) ── */
  const kbdOn = (n: number) => {
    setPressedKbd((prev) => {
      if (prev.has(n)) return prev
      const s = new Set(prev)
      s.add(n)
      return s
    })
    setLastEvent(`клав: нажатие ${noteName(n)} vel 100`)
  }
  const kbdOff = (n: number) => {
    setPressedKbd((prev) => {
      if (!prev.has(n)) return prev
      const s = new Set(prev)
      s.delete(n)
      return s
    })
    setLastEvent(`клав: отпускание ${noteName(n)}`)
  }

  /* ── управление MIDI-файлом ── */
  const stopFile = (auto = false) => {
    setFileState('stopped')
    posRef.current = 0
    stepRef.current = 0
    accRef.current = 0
    fileNotesRef.current = []
    fileSigRef.current = ''
    setPressedFile(new Set())
    setFilePos(0)
    if (auto) setLastEvent('файл: воспроизведение завершено')
  }
  const togglePlay = () => {
    if (!fileIdRef.current) return
    if (playRef.current === 'playing') {
      setFileState('paused')
      setLastEvent('файл: пауза')
    } else {
      setFileState('playing')
      setLastEvent('файл: воспроизведение')
    }
  }
  useEffect(() => {
    togglePlayRef.current = togglePlay
  })

  /* ── клавиатура компьютера ── */
  useEffect(() => {
    const down = (e: KeyboardEvent) => {
      if (dialogRef.current) return
      if (e.repeat || e.metaKey || e.ctrlKey || e.altKey) return
      if (e.code === 'Space') {
        e.preventDefault()
        togglePlayRef.current()
        return
      }
      const n = KEY_MAP[e.code]
      if (n === undefined) return
      e.preventDefault()
      kbdOn(n)
    }
    const up = (e: KeyboardEvent) => {
      const n = KEY_MAP[e.code]
      if (n === undefined) return
      kbdOff(n)
    }
    window.addEventListener('keydown', down)
    window.addEventListener('keyup', up)
    return () => {
      window.removeEventListener('keydown', down)
      window.removeEventListener('keyup', up)
    }
  }, [])

  /* ── симуляция входящих MIDI-сообщений ── */
  useEffect(() => {
    if (!midiDevId || midiDevId === 'none' || midiErr) {
      simRef.current = []
      simSigRef.current = ''
      return
    }
    const id = setInterval(() => {
      const now = Date.now()
      let arr = simRef.current.filter((n) => n.until > now)
      if (arr.length < 3 && Math.random() < 0.05) {
        const base = 48 + Math.floor(Math.random() * 25)
        const vel = 40 + Math.floor(Math.random() * 80)
        arr = [...arr, { note: base, until: now + 300 + Math.random() * 800, vel }]
        if (Math.random() < 0.25 && arr.length < 4) {
          arr = [...arr, { note: base + (Math.random() < 0.5 ? 4 : 7), until: now + 250 + Math.random() * 500, vel: Math.max(1, vel - 10) }]
        }
      }
      simRef.current = arr
      const sig = arr.map((n) => n.note).sort((a, b) => a - b).join(',')
      if (sig !== simSigRef.current) {
        simSigRef.current = sig
        setPressedMidi(new Set(arr.map((n) => n.note)))
        const newest = arr[arr.length - 1]
        if (newest) setLastEvent(`MIDI: нажатие ${noteName(newest.note)} vel ${newest.vel}`)
      }
    }, 60)
    return () => {
      clearInterval(id)
      simRef.current = []
      simSigRef.current = ''
    }
  }, [midiDevId, midiErr])

  /* ── главный такт: воспроизведение, измерители, cpu, запись ── */
  useEffect(() => {
    const id = setInterval(() => {
      const now = Date.now()

      /* воспроизведение MIDI-файла */
      if (playRef.current === 'playing' && fileIdRef.current) {
        posRef.current = Math.min(posRef.current + 0.05, durRef.current)
        accRef.current += 0.05
        while (accRef.current >= STEP) {
          accRef.current -= STEP
          const st = stepRef.current
          const note = PATTERN[st % PATTERN.length]
          const held = fileNotesRef.current
          if (held.length < 60) {
            fileNotesRef.current = [...held, { note, until: now + 220, vel: 90 }]
            if (st % 8 === 0) {
              fileNotesRef.current = [
                ...fileNotesRef.current,
                { note: note + 12, until: now + 1900, vel: 60 },
                { note: note + 19, until: now + 1900, vel: 55 },
              ]
            }
          }
          stepRef.current = (st + 1) % PATTERN.length
          setLastEvent(`файл: ${noteName(note)} vel 90`)
        }
        if (posRef.current >= durRef.current && durRef.current > 0) {
          stopFile(true)
        } else {
          setFilePos(Math.round(posRef.current * 10) / 10)
        }
      }

      /* снятие отыгранных нот файла */
      const alive = fileNotesRef.current.filter((n) => n.until > now)
      if (alive.length !== fileNotesRef.current.length) fileNotesRef.current = alive
      const fsig = alive.map((n) => n.note).sort((a, b) => a - b).join(',')
      if (fsig !== fileSigRef.current) {
        fileSigRef.current = fsig
        setPressedFile(new Set(alive.map((n) => n.note)))
      }

      /* измерители и cpu */
      const total = kbdRef.current.size + simRef.current.length + fileNotesRef.current.length
      const devOk = !!audioRef.current && audioRef.current !== 'none'
      const mv = masterRef.current / 100
      const base = devOk && total > 0 ? Math.min(0.95, 0.16 + 0.2 * total) * mv : 0
      const tL = base * (0.85 + Math.random() * 0.3)
      const tR = base * (0.8 + Math.random() * 0.32)
      setMeterL((p) => {
        let nx = p + (tL - p) * 0.3
        if (nx < 0.002) nx = 0
        const r = Math.round(nx * 1000) / 1000
        return r === p ? p : r
      })
      setMeterR((p) => {
        let nx = p + (tR - p) * 0.3
        if (nx < 0.002) nx = 0
        const r = Math.round(nx * 1000) / 1000
        return r === p ? p : r
      })
      const ctarget = Math.min(
        99,
        3 + total * 1.3 + (playRef.current === 'playing' ? 2 : 0) + (recRef.current.recording ? 1 : 0),
      )
      setCpu((p) => {
        const nx = Math.round(p + (ctarget - p) * 0.15)
        return nx === p ? p : nx
      })

      /* запись в аудиофайл */
      if (recRef.current.recording) {
        recRef.current.time += 0.05
        recRef.current.size += recRef.current.format === 'MIDI' ? 2500 * 0.05 : 110000 * 0.05
        setRecTime(Math.floor(recRef.current.time))
        setRecSize(Math.round(recRef.current.size))
      }
    }, 50)
    return () => clearInterval(id)
  }, [])

  /* ── обработчик параметров ADSR ── */
  const adjAdsr = (k: keyof Adsr, dir: 1 | -1) => {
    setAdsr((prev) => {
      if (k === 's') {
        const v = Math.max(0, Math.min(100, Math.round(prev.s * 100) + dir * 5)) / 100
        return { ...prev, s: v }
      }
      const v = Math.max(0.001, Math.min(2, Math.round((prev[k] + dir * 0.005) * 1000) / 1000))
      return { ...prev, [k]: v }
    })
    setAdsrPreset('Своё')
  }
  const applyAdsr = (name: AdsrName) => {
    setAdsr({ ...ADSR_PRESETS[name] })
    setAdsrPreset(name)
  }
  const adsrText = (k: keyof Adsr) =>
    k === 's' ? `${Math.round(adsr.s * 100)}%` : `${adsr[k].toFixed(3)} c`

  /* ── запись ── */
  const startRec = () => {
    if (recRef.current.recording) return
    const d = new Date()
    const p = (n: number) => String(n).padStart(2, '0')
    const stamp = `${d.getFullYear()}${p(d.getMonth() + 1)}${p(d.getDate())}_${p(d.getHours())}${p(d.getMinutes())}${p(d.getSeconds())}`
    const ext = recFormat === 'MIDI' ? 'mid' : recFormat.toLowerCase()
    setRecFile(`rec_${stamp}.${ext}`)
    recRef.current.time = 0
    recRef.current.size = 0
    setRecTime(0)
    setRecSize(0)
    setRecording(true)
    setLastEvent(`запись: старт (${recFormat})`)
  }
  const stopRec = () => {
    if (!recRef.current.recording) return
    setRecording(false)
    setRecSaved({ name: recFile, size: Math.max(1, recRef.current.size) })
    setLastEvent(`запись: сохранено ${recFile}`)
  }
  const cancelRec = () => {
    if (!recRef.current.recording) return
    setRecording(false)
    setLastEvent('запись: отменена')
  }

  /* ── диалоги ── */
  const itemsFor = (kind: DialogKind): DialogItem[] => {
    switch (kind) {
      case 'midi-dev':
        return MIDI_DEVICES.map((d) => ({ id: d.id, label: d.label, hint: d.hint }))
      case 'midi-file':
        return MIDI_FILES.map((f) => ({ id: f.id, label: f.label, hint: fmtTime(f.dur) }))
      case 'sf2':
        return SF2_FILES.map((f) => ({ id: f.id, label: f.label, hint: f.hint }))
      case 'preset':
        return GM_PRESETS.map((p) => ({ id: p, label: p }))
      case 'audio-out':
        return AUDIO_DEVICES.map((d) => ({ id: d.id, label: d.label, hint: d.params }))
      case 'folder':
        return FOLDERS.map((f) => ({ id: f, label: f }))
    }
  }
  const currentFor = (kind: DialogKind): string | null => {
    switch (kind) {
      case 'midi-dev': return midiDevId ?? 'none'
      case 'midi-file': return fileId
      case 'sf2': return sf2Id
      case 'preset': return preset
      case 'audio-out': return audioDevId ?? 'none'
      case 'folder': return recFolder
    }
  }
  const onDialogSelect = (id: string) => {
    switch (dialog) {
      case 'midi-dev':
        if (id === 'none') {
          setMidiDevId(null)
          setMidiErr(null)
        } else if (id === 'busy') {
          setMidiDevId(id)
          setMidiErr('открытие не удалось: устройство занято другим процессом (EBUSY)')
        } else {
          setMidiDevId(id)
          setMidiErr(null)
        }
        break
      case 'midi-file': {
        stopFile()
        const f = MIDI_FILES.find((x) => x.id === id)
        if (f) {
          setFileId(id)
          setLastEvent(`файл: выбран ${f.label}`)
        }
        break
      }
      case 'sf2':
        if (id === 'none') {
          setSf2Id(null)
          setSf2Err('файл не выбран')
        } else if (id === 'broken') {
          setSf2Id(id)
          setSf2Err('ошибка загрузки: файл повреждён (неверный RIFF-заголовок)')
        } else if (id === 'missing') {
          setSf2Id(id)
          setSf2Err('ошибка загрузки: файл отсутствует на диске')
        } else {
          setSf2Id(id)
          setSf2Err(null)
          setPreset(GM_PRESETS[0])
        }
        break
      case 'preset':
        setPreset(id)
        setLastEvent(`SF2: пресет ${id}`)
        break
      case 'audio-out':
        setAudioDevId(id === 'none' ? null : id)
        break
      case 'folder':
        setRecFolder(id)
        break
    }
    setDialog(null)
  }

  /* ── строки состояний ── */
  const fileStateText =
    fileState === 'playing' ? (
      <span className="text-green-500">воспроизведение</span>
    ) : fileState === 'paused' ? (
      <span className="text-amber-300">пауза</span>
    ) : (
      <span className="text-neutral-600">остановлен</span>
    )
  const engineSummary =
    engine === 'synth'
      ? `synth · ${WAVE_RU[wave]} · ADSR ${adsrPreset}`
      : `SF2 · ${sf2Ok ? (sf2Sel?.label ?? '—') : '—'}`

  const pressedNames = (set: ReadonlySet<number>) =>
    set.size > 0 ? (
      <span className="text-amber-300">{Array.from(set).sort((a, b) => a - b).map(noteName).join(' ')}</span>
    ) : (
      <span className="text-neutral-700">—</span>
    )

  /* ═══════════════════════════ рендер ═══════════════════════════ */
  return (
    <div className="h-dvh bg-neutral-900 p-2 sm:p-3">
      <div className="mx-auto flex h-full max-w-6xl flex-col overflow-hidden rounded-md border border-neutral-700 bg-black font-mono text-xs text-neutral-200 sm:text-sm">
        {/* строка заголовка окна (эмуляция Konsole) */}
        <div className="flex shrink-0 items-center gap-2 border-b border-black bg-[#2b2b2b] px-3 py-1.5 text-[11px] text-neutral-400">
          <span aria-hidden>✎</span>
          <span className="flex-1 truncate text-center text-neutral-300">
            midi_processor : design — Konsole
          </span>
          <span aria-hidden className="text-neutral-500">⌄</span>
          <span aria-hidden className="flex h-4 w-4 items-center justify-center rounded-full bg-red-600 text-[9px] text-white">
            ✕
          </span>
        </div>

        {/* прокручиваемое содержимое терминала */}
        <main className="tui-scroll flex-1 space-y-4 overflow-y-auto px-3 py-3 sm:px-5">
          {/* заголовок приложения */}
          <div className="flex items-center gap-3 text-[11px] text-neutral-500">
            <span className="h-px flex-1 bg-neutral-800" />
            <span className="tracking-[0.25em]">midi_processor 0.2.0</span>
            <span className="h-px flex-1 bg-neutral-800" />
          </div>

          {/* ════ ИСТОЧНИКИ ════ */}
          <section aria-label="Источники MIDI" className="space-y-2.5">
            <Section title="ИСТОЧНИКИ MIDI" />

            <div>
              <Row label="Клавиатура">
                {pressedNames(pressedKbd)}
                <span className="text-[10px] text-neutral-600">
                  [Z–M · Q–P] играть · клик по клавишам
                </span>
              </Row>
              <div className="mt-1.5 sm:pl-36">
                <Piano pressed={pressedKbd} onDown={kbdOn} onUp={kbdOff} />
              </div>
            </div>

            <Row label="MIDI-вход">
              <TButton onClick={() => setDialog('midi-dev')} title="выбрать MIDI-вход">
                MIDI-вход
              </TButton>
              {midiDevName ? (
                <span className="text-neutral-100">{midiDevName}</span>
              ) : (
                <span className="text-neutral-600">(не выбрано)</span>
              )}
              {midiErr ? (
                <span className="text-red-500">{midiErr}</span>
              ) : midiDevId && midiDevId !== 'none' ? (
                <span className="text-green-500">открыто</span>
              ) : null}
            </Row>
            <Row label="→ ноты">{pressedNames(pressedMidiView)}</Row>

            <Row label="MIDI-файл">
              <TButton onClick={() => setDialog('midi-file')} title="выбрать MIDI-файл">
                MIDI-файл
              </TButton>
              {fileSel ? (
                <span className="text-neutral-100">{fileSel.label}</span>
              ) : (
                <span className="text-neutral-600">(файл не выбран)</span>
              )}
              <TButton onClick={() => setFileState('playing')} disabled={!fileSel || fileState === 'playing'}>
                Старт
              </TButton>
              <TButton
                onClick={() => {
                  setFileState('paused')
                  setLastEvent('файл: пауза')
                }}
                disabled={fileState !== 'playing'}
                active={fileState === 'paused'}
              >
                Пауза
              </TButton>
              <TButton onClick={() => stopFile(false)} disabled={fileState === 'stopped' && filePos === 0}>
                Стоп
              </TButton>
            </Row>
            {fileSel && (
              <Row label="позиция">
                <Bar pos={filePos / Math.max(1, fileDur)} />
                <span className="tabular-nums text-neutral-300">
                  {fmtTime(filePos)} / {fmtTime(fileDur)}
                </span>
                {fileStateText}
                {pressedNames(pressedFile)}
              </Row>
            )}
          </section>

          {/* ════ ОБРАБОТЧИК ════ */}
          <section aria-label="Обработчик" className="space-y-2.5">
            <Section
              title="ОБРАБОТЧИК"
              right={
                <span className="flex gap-1.5">
                  <TButton active={engine === 'synth'} onClick={() => setEngine('synth')}>
                    Synth
                  </TButton>
                  <TButton active={engine === 'sf2'} onClick={() => setEngine('sf2')}>
                    Sound Font
                  </TButton>
                </span>
              }
            />

            {engine === 'synth' ? (
              <>
                <Row label="Форма сигнала">
                  {(Object.keys(WAVE_RU) as Wave[]).map((w) => (
                    <TButton key={w} active={wave === w} onClick={() => setWave(w)}>
                      {WAVE_RU[w]}
                    </TButton>
                  ))}
                </Row>
                <div className="sm:pl-36">
                  <WaveAscii wave={wave} />
                </div>
                <Row label="ADSR">
                  <span className="text-neutral-500">пресет:</span>
                  {(Object.keys(ADSR_PRESETS) as AdsrName[]).map((n) => (
                    <TButton key={n} active={adsrPreset === n} onClick={() => applyAdsr(n)}>
                      {n}
                    </TButton>
                  ))}
                  {adsrPreset === 'Своё' && <span className="text-amber-300">Своё</span>}
                </Row>
                <Row label="значения">
                  <div className="grid w-full grid-cols-2 gap-x-6 gap-y-1.5 sm:grid-cols-4">
                    {(
                      [
                        ['a', 'Attack'],
                        ['d', 'Decay'],
                        ['s', 'Sustain'],
                        ['r', 'Release'],
                      ] as const
                    ).map(([k, label]) => (
                      <div key={k} className="flex items-baseline gap-1.5">
                        <span className="w-16 text-neutral-500">{label}:</span>
                        <TButton onClick={() => adjAdsr(k, -1)} title={`уменьшить ${label}`}>
                          −
                        </TButton>
                        <span className="min-w-[6ch] text-left tabular-nums text-amber-300">
                          {adsrText(k)}
                        </span>
                        <TButton onClick={() => adjAdsr(k, 1)} title={`увеличить ${label}`}>
                          +
                        </TButton>
                      </div>
                    ))}
                  </div>
                </Row>
              </>
            ) : (
              <>
                <Row label="Файл Sound Font">
                  <TButton onClick={() => setDialog('sf2')} title="выбрать файл Sound Font">
                    Файл Sound Font
                  </TButton>
                  {sf2Sel && sf2Id !== 'none' ? (
                    <span className="text-neutral-100">{sf2Sel.label}</span>
                  ) : (
                    <span className="text-neutral-600">(файл не выбран)</span>
                  )}
                  {sf2Ok && sf2Sel?.hint && <span className="text-neutral-600">{sf2Sel.hint}</span>}
                  {sf2Ok ? (
                    <span className="text-green-500">OK</span>
                  ) : (
                    <span className="text-red-500">{sf2Err ?? 'файл не выбран'}</span>
                  )}
                </Row>
                <Row label="Пресет">
                  <TButton onClick={() => setDialog('preset')} disabled={!sf2Ok} title="выбрать пресет">
                    Пресет
                  </TButton>
                  <span className={sf2Ok ? 'text-neutral-100' : 'text-neutral-700'}>
                    {sf2Ok ? preset : '—'}
                  </span>
                </Row>
              </>
            )}
          </section>

          {/* ════ ВЫХОД ════ */}
          <section aria-label="Выход" className="space-y-2.5">
            <Section title="ВЫХОД" />

            <Row label="Аудио выход">
              <TButton onClick={() => setDialog('audio-out')} title="выбрать аудио выход">
                Аудио выход
              </TButton>
              {audioSel && audioDevId !== 'none' ? (
                <>
                  <span className="text-neutral-100">{audioSel.label}</span>
                  <span className="text-neutral-600">{audioSel.params}</span>
                  <span className="text-green-500">OK</span>
                </>
              ) : (
                <span className="text-red-500">нет аудио устройства</span>
              )}
            </Row>
            <Row label="L">
              <span className="select-none tracking-tight">
                <Meter level={meterL} />
              </span>
              <span className="w-20 whitespace-nowrap tabular-nums text-neutral-300">{dbfs(meterL)} dBFS</span>
            </Row>
            <Row label="R">
              <span className="select-none tracking-tight">
                <Meter level={meterR} />
              </span>
              <span className="w-20 whitespace-nowrap tabular-nums text-neutral-300">{dbfs(meterR)} dBFS</span>
            </Row>
            <Row label="Мастер">
              <TButton onClick={() => setMaster((m) => Math.max(0, m - 5))} title="тише">
                −
              </TButton>
              <span className="select-none">
                <Meter level={master / 100} cols={20} />
              </span>
              <span className="w-10 tabular-nums text-amber-300">{master}%</span>
              <TButton onClick={() => setMaster((m) => Math.min(100, m + 5))} title="громче">
                +
              </TButton>
            </Row>

            <div className="pt-1">
              <Section title="АУДИО-ФАЙЛ" />
            </div>

            <Row label="Папка">
              <TButton onClick={() => setDialog('folder')} title="выбрать папку для файлов">
                Папка для файлов
              </TButton>
              <span className="text-neutral-100">{recFolder}</span>
            </Row>
            <Row label="Формат">
              {FORMATS.map((f) => (
                <TButton key={f} active={recFormat === f} onClick={() => setRecFormat(f)}>
                  {f}
                </TButton>
              ))}
              <span className="text-[10px] text-neutral-600">
                {recFormat === 'MIDI' ? 'запись MIDI-событий' : 'запись аудиопотока'}
              </span>
            </Row>
            <Row label="Запись">
              <TButton onClick={startRec} disabled={recording} title="начать запись">
                Старт
              </TButton>
              <TButton onClick={stopRec} disabled={!recording} title="остановить и сохранить">
                Стоп
              </TButton>
              <TButton onClick={cancelRec} disabled={!recording} danger title="отменить запись">
                Отмена
              </TButton>
              {recording ? (
                <span className="text-red-500">
                  <span className="animate-pulse">●</span> REC {fmtTime(recTime)} ·{' '}
                  {fmtBytes(recSize)} · {recFolder}/{recFile}
                </span>
              ) : recSaved ? (
                <span className="text-green-500">
                  сохранено: {recFolder}/{recSaved.name} · {fmtBytes(recSaved.size)}
                </span>
              ) : (
                <span className="text-neutral-600">— записей ещё не было —</span>
              )}
            </Row>
          </section>
        </main>

        {/* строка состояния (липкий низ) */}
        <StatusBar>
          <span>
            голоса <span className="text-amber-300">{voices}</span>/64
          </span>
          <span>
            cpu{' '}
            <span className={cpu > 80 ? 'text-red-500' : 'text-amber-300'}>{cpu}%</span>
          </span>
          <span>xrun 0</span>
          <span className="min-w-0 truncate">
            клав: <span className="text-neutral-200">{lastEvent ?? '—'}</span>
          </span>
          <span className="ml-auto min-w-0 truncate">
            движок: <span className="text-amber-300">{engineSummary}</span>
          </span>
          <span aria-hidden className="cursor-blink text-amber-300">
            ▌
          </span>
        </StatusBar>
      </div>

      {/* модальное TUI-окно выбора */}
      {dialog && (
        <TDialog
          title={
            {
              'midi-dev': 'ВЫБОР MIDI-ВХОДА',
              'midi-file': 'ВЫБОР MIDI-ФАЙЛА',
              sf2: 'ВЫБОР ФАЙЛА SOUND FONT',
              preset: 'ВЫБОР ПРЕСЕТА',
              'audio-out': 'ВЫБОР АУДИО-ВЫХОДА',
              folder: 'ВЫБОР ПАПКИ',
            }[dialog]
          }
          items={itemsFor(dialog)}
          current={currentFor(dialog)}
          onSelect={onDialogSelect}
          onCancel={() => setDialog(null)}
        />
      )}
    </div>
  )
}
