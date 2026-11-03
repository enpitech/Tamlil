import type { CommandRunInput, On } from 'claude-code'
import { expect, test } from 'claude-code/testing'

import {
  cleanTranscript,
  pickModel,
  recordArgs,
  speakable,
  tamlilModelsDir,
  whisperArgs,
} from '../hooks/lib'

const HOME = '/Users/me'
const MODELS = tamlilModelsDir(HOME)
const LISTEN: CommandRunInput = {
  command: 'listen',
  args: '',
  origin: { kind: 'composer' },
  presentation: { isFullscreen: false, columns: 80 },
}

test('picks the best ivrit.ai model Tamlil downloaded', async () => {
  expect(pickModel(['ggml-base.bin', 'ggml-ivrit-large-v3.bin', 'ggml-ivrit-large-v3-turbo.bin'])).toBe(
    'ggml-ivrit-large-v3-turbo.bin',
  )
  expect(pickModel(['ggml-ivrit-ai-yi-whisper-large-v3-turbo-ggml-model.bin'])).toBe(
    'ggml-ivrit-ai-yi-whisper-large-v3-turbo-ggml-model.bin',
  )
  // Tamlil's default Hebrew model is the Zipformer, not a whisper .bin.
  expect(pickModel(['registry.json', 'ivrit-zipformer-66m-he', 'ggml-base.bin'])).toBe(undefined)
})

test('records 16 kHz mono and stops on silence and length', async () => {
  expect(recordArgs('/t/a.wav', 2, 120)).toEqual([
    '-q', '-c', '1', '-r', '16000', '-b', '16', '/t/a.wav',
    'silence', '1', '0.1', '1%', '1', '2', '1%',
    'trim', '0', '120',
  ])
  expect(recordArgs('/t/a.wav', 0, 0)).toEqual(['-q', '-c', '1', '-r', '16000', '-b', '16', '/t/a.wav'])
  expect(whisperArgs('/m.bin', 'he', '/t/a.wav')).toEqual(['-m', '/m.bin', '-l', 'he', '-f', '/t/a.wav', '-nt', '-np'])
})

test('cleans whisper output and markdown for speech', async () => {
  expect(cleanTranscript(' [BLANK_AUDIO]\n שלום, תוסיף בדיקה\n (מוזיקה) לקובץ הזה \n')).toBe(
    'שלום, תוסיף בדיקה לקובץ הזה',
  )
  expect(cleanTranscript('[BLANK_AUDIO]\n')).toBe('')
  expect(speakable('## כותרת\nראה `lib.ts` ו-[קישור](https://x.y)\n```ts\ncode()\n```\nסוף')).toBe(
    'כותרת ראה lib.ts ו-קישור סוף',
  )
})

type Run = { argv: readonly string[] }

/** Stands for the Mac: which finds both tools, ls lists Tamlil's models. */
function mac(on: On, ran: string[][], models: string) {
  on('process.run', (_$, e: Run) => {
    ran.push([...e.argv])
    const [bin, ...rest] = e.argv
    const ok = (stdout = '') => ({ value: { exitCode: 0, stdout, stderr: '', isStdoutTruncated: false, isStderrTruncated: false } })
    if (bin === '/usr/bin/which') return ok(`/opt/homebrew/bin/${rest[0]}\n`)
    if (bin === '/bin/ls') return ok(models)
    if (bin === '/opt/homebrew/bin/whisper-cli') return ok(' שלום קלוד, תסכם את הפגישה\n')
    return ok()
  })
  on('env.get', (_$, e: { name: string }) => ({ value: e.name === 'HOME' ? HOME : '/tmp/' }))
  on('command.register', () => ({ value: { command: 'listen' } }))
  on('session.start', (_, e) => ({ cwd: e.cwd }))
}

test('/listen records, transcribes with the ivrit model and fills the prompt', async ($, on) => {
  const ran: string[][] = []
  const spawned: string[][] = []
  const filled: string[] = []
  let cleaned!: () => void
  const finished = new Promise<void>(resolve => (cleaned = resolve))
  mac(on, ran, 'registry.json\nggml-ivrit-large-v3-turbo.bin\n')
  on('process.spawn', async function* (_$, e: Run) {
    spawned.push([...e.argv])
    return { value: { code: 0, signal: null } }
  })
  on('prompt.read', () => ({ value: { text: '', cursor: 0 } }))
  on('prompt.fill', (_$, e: { text: string }) => {
    filled.push(e.text)
    return { isFilled: true, text: e.text, cursor: e.text.length }
  })
  on('ui.status', (_$, e: { text?: string }) => {
    if (e.text === undefined) cleaned()
    return { value: undefined }
  })

  await $.session.start({ cwd: '/', surface: 'terminal', isInteractive: true })
  const out = await $.command.run(LISTEN)
  expect(out.text).toContain('Listening')

  await finished
  expect(spawned[0]?.[0]).toBe('/opt/homebrew/bin/rec')
  const whisper = ran.find(argv => argv[0] === '/opt/homebrew/bin/whisper-cli')
  expect(whisper).toEqual([
    '/opt/homebrew/bin/whisper-cli', '-m', `${MODELS}/ggml-ivrit-large-v3-turbo.bin`,
    '-l', 'he', '-f', expect.stringMatching(/^\/tmp\/tamlil-voice-\d+\.wav$/), '-nt', '-np',
  ])
  expect(filled).toEqual(['שלום קלוד, תסכם את הפגישה'])
  expect(ran.some(argv => argv[0] === '/bin/rm')).toBe(true)
})

test('/listen says how to get a model when Tamlil has none', async ($, on) => {
  const ran: string[][] = []
  mac(on, ran, 'registry.json\nivrit-zipformer-66m-he\n')

  await $.session.start({ cwd: '/', surface: 'terminal', isInteractive: true })
  const out = await $.command.run(LISTEN)
  expect(out.text).toContain('No Hebrew whisper model found')
  expect(out.text).toContain('ivrit-ai/whisper-large-v3-turbo-ggml')
})
