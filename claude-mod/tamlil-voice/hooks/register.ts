import type { EngineInterface, Register } from 'claude-code'

import {
  RECOMMENDED_MODEL_URL,
  cleanTranscript,
  pickModel,
  recordArgs,
  speakable,
  tamlilModelsDir,
  whisperArgs,
} from './lib'

/** The recording in progress: its file, so /listen again can stop it. */
let recording: { wav: string } | undefined
/** Set when a spoken prompt was handed over, so only its answer is spoken. */
let answerAloud = false

export const register: Register = (on, options) => {
  const language = String(options.language || 'he')
  const modelPath = String(options.modelPath ?? '').trim()
  const autoSend = options.autoSend === true
  const speakReplies = options.speakReplies === true
  const voice = String(options.voice || 'Carmit')
  const silenceSeconds = Number(options.silenceSeconds ?? 2)
  const maxSeconds = Number(options.maxSeconds ?? 120)

  on('session.start', async ($, e, next) => {
    await $.command.register({
      name: 'listen',
      description: 'Speak Hebrew: record, transcribe locally with Tamlil (again to stop)',
      immediate: true,
    })
    return next(e)
  })

  on('command.run', { command: 'listen' }, async $ => {
    if (recording) {
      await $.process.run(['/usr/bin/pkill', '-INT', '-f', recording.wav])
      return { text: 'Recording stopped, transcribing.' }
    }

    const rec = await findBinary($, 'rec')
    const whisper = await findBinary($, 'whisper-cli')
    if (!rec || !whisper) {
      return {
        text:
          'tamlil-voice needs sox and whisper.cpp. Install them with:\n' +
          '  brew install sox whisper-cpp',
      }
    }

    const model = await resolveModel($, modelPath)
    if (!model) {
      return {
        text:
          'No Hebrew whisper model found. In Tamlil, open Settings → Models and download ' +
          '"ivrit.ai whisper large-v3 turbo", or set "Model file" in /config to a .bin file. ' +
          `Direct download: ${RECOMMENDED_MODEL_URL}`,
      }
    }

    const tmp = ((await $.env.get('TMPDIR')) || '/tmp').replace(/\/$/, '')
    const wav = `${tmp}/tamlil-voice-${Date.now()}.wav`
    recording = { wav }
    $.ui.status('🎙 Listening… (/listen to stop)')

    void (async () => {
      try {
        const child = $.process.spawn({ argv: [rec, ...recordArgs(wav, silenceSeconds, maxSeconds)] })
        for await (const _chunk of child) {
          // sox is quiet (-q); the loop is the recorder's life.
        }
        recording = undefined
        $.ui.status('✍️ Transcribing…')
        const ran = await $.process.run([whisper, ...whisperArgs(model, language, wav)], {
          timeoutMs: 300_000,
        })
        const text = ran.exitCode === 0 ? cleanTranscript(ran.stdout) : ''
        if (ran.exitCode !== 0) {
          $.ui.toast(`Transcription failed: ${ran.stderr.trim().split('\n').pop() ?? 'unknown error'}`)
        } else if (!text) {
          $.ui.toast('Heard nothing. Check the microphone permission of your terminal.')
        } else if (autoSend) {
          answerAloud = true
          await $.prompt.submit({ text, asUser: true })
        } else {
          answerAloud = true
          const box = await $.prompt.read()
          await $.prompt.fill({ text: box.text.trim() ? ` ${text}` : text, mode: 'append' })
        }
      } catch (error) {
        $.ui.toast(`tamlil-voice: ${error instanceof Error ? error.message : String(error)}`)
      } finally {
        recording = undefined
        $.ui.status(undefined)
        await $.process.run(['/bin/rm', '-f', wav]).catch(() => undefined)
      }
    })()

    return { text: 'Listening. Speak, then pause (or run /listen again).' }
  })

  on('turn.complete', async ($, e, next) => {
    const done = await next(e)
    if (speakReplies && answerAloud && e.agentId === undefined && !e.isAborted && e.answer) {
      answerAloud = false
      void $.audio.speak(speakable(e.answer), { voice }).catch(error => {
        $.ui.toast(`Could not speak with voice "${voice}": ${String(error)}`)
      })
    }
    return done
  })
}

/** The binary's path: on PATH, else where Homebrew puts it on either Mac. */
async function findBinary($: EngineInterface, name: string): Promise<string | undefined> {
  const which = await $.process.run(['/usr/bin/which', name])
  if (which.exitCode === 0 && which.stdout.trim()) return which.stdout.trim()
  for (const dir of ['/opt/homebrew/bin', '/usr/local/bin']) {
    const test = await $.process.run(['/bin/test', '-x', `${dir}/${name}`])
    if (test.exitCode === 0) return `${dir}/${name}`
  }
  return undefined
}

/** The configured model, else the best ivrit.ai model in Tamlil's folder. */
async function resolveModel($: EngineInterface, configured: string): Promise<string | undefined> {
  const home = (await $.env.get('HOME')) ?? ''
  if (configured) return configured.replace(/^~(?=\/)/, home)
  const dir = tamlilModelsDir(home)
  const listing = await $.process.run(['/bin/ls', '-1', dir])
  if (listing.exitCode !== 0) return undefined
  const file = pickModel(listing.stdout.split('\n'))
  return file ? `${dir}/${file}` : undefined
}
