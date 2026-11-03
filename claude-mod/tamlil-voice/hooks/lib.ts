// Pure helpers of the tamlil-voice mod: no `$`, so the tests call them directly.

/** Where Tamlil keeps its models on macOS (Tauri app data dir + "models"). */
export const tamlilModelsDir = (home: string): string =>
  `${home}/Library/Application Support/com.enpitech.tamlil/models`

/** Hebrew whisper.cpp models Tamlil's registry can download, best first. */
export const PREFERRED_MODELS = [
  'ggml-ivrit-large-v3-turbo.bin',
  'ggml-ivrit-large-v3-turbo-q4_0.bin',
  'ggml-ivrit-large-v3.bin',
  'ggml-ivrit-v2-d4.bin',
] as const

/** The URL of the recommended model, for the message when none is found. */
export const RECOMMENDED_MODEL_URL =
  'https://huggingface.co/ivrit-ai/whisper-large-v3-turbo-ggml/resolve/main/ggml-model.bin'

/**
 * Chooses the model file from a folder listing: a preferred ivrit.ai model,
 * else any other ggml-ivrit*.bin, else undefined. Tamlil's default Hebrew
 * model is the Zipformer (ONNX, not whisper), so a folder may hold none.
 */
export function pickModel(files: readonly string[]): string | undefined {
  const names = new Set(files.map(f => f.trim()).filter(Boolean))
  const preferred = PREFERRED_MODELS.find(name => names.has(name))
  if (preferred) return preferred
  return [...names].filter(n => /^ggml-ivrit.*\.bin$/.test(n)).sort()[0]
}

/** sox `rec` arguments: 16 kHz mono 16-bit WAV, as whisper.cpp wants it. */
export function recordArgs(wav: string, silenceSeconds: number, maxSeconds: number): string[] {
  const args = ['-q', '-c', '1', '-r', '16000', '-b', '16', wav]
  // Trim silence before speech, then stop after `silenceSeconds` of quiet.
  if (silenceSeconds > 0) args.push('silence', '1', '0.1', '1%', '1', String(silenceSeconds), '1%')
  if (maxSeconds > 0) args.push('trim', '0', String(maxSeconds))
  return args
}

/** whisper-cli arguments: plain text on stdout, no timestamps or progress. */
export function whisperArgs(model: string, language: string, wav: string): string[] {
  return ['-m', model, '-l', language, '-f', wav, '-nt', '-np']
}

/** Joins whisper's lines and drops its markers ([BLANK_AUDIO], (music), …). */
export function cleanTranscript(stdout: string): string {
  return stdout
    .split('\n')
    .map(line => line.replace(/\[[^\]]*\]|\([^)]*\)/g, '').trim())
    .filter(Boolean)
    .join(' ')
    .replace(/\s+/g, ' ')
    .trim()
}

/** Markdown to plain speech: no code blocks, markup, or bare URLs. */
export function speakable(markdown: string, limit = 4000): string {
  return markdown
    .replace(/```[\s\S]*?```/g, ' ')
    .replace(/`([^`]*)`/g, '$1')
    .replace(/!?\[([^\]]*)\]\([^)]*\)/g, '$1')
    .replace(/https?:\/\/\S+/g, ' ')
    .replace(/[*_#>|~]+/g, ' ')
    .replace(/\s+/g, ' ')
    .trim()
    .slice(0, limit)
}
