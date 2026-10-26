'use client';

import { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { toast } from 'sonner';
import { Cpu } from 'lucide-react';
import { useConfig } from '@/contexts/ConfigContext';
import type { RoutedLanguageEvent } from '@/lib/language-routes';
import { LANGUAGES } from '@/constants/languages';

interface RawModel {
  name: string;
  status: unknown;
  display_name?: string;
  languages?: string[];
}

interface Option {
  key: string;
  provider: 'localWhisper' | 'parakeet';
  model: string;
  label: string;
}

/**
 * Shows which transcription model the next (or current) recording uses, with a
 * switch among the models already on disk. While recording it reports the model
 * the language route switched to, and the switch is locked until the recording ends.
 */
export function TranscriptionEngineBadge({ isRecording }: { isRecording: boolean }) {
  const { transcriptModelConfig, setTranscriptModelConfig } = useConfig();
  const [options, setOptions] = useState<Option[]>([]);
  const [routed, setRouted] = useState<RoutedLanguageEvent | null>(null);
  const [saving, setSaving] = useState(false);

  const load = useCallback(async () => {
    const out: Option[] = [];
    try {
      const whisper = await invoke<RawModel[]>('whisper_get_available_models');
      for (const m of whisper.filter((m) => m.status === 'Available')) {
        out.push({ key: `localWhisper:${m.name}`, provider: 'localWhisper', model: m.name, label: `Whisper · ${m.display_name && m.display_name !== m.name ? m.display_name : m.name}` });
      }
    } catch { /* engine not ready */ }
    try {
      const parakeet = await invoke<RawModel[]>('parakeet_get_available_models');
      for (const m of parakeet.filter((m) => m.status === 'Available')) {
        out.push({ key: `parakeet:${m.name}`, provider: 'parakeet', model: m.name, label: m.display_name && m.display_name !== m.name ? m.display_name : `Parakeet · ${m.name}` });
      }
    } catch { /* engine not ready */ }
    setOptions(out);
  }, []);

  useEffect(() => {
    void load();
    const done = listen('model-download-complete', () => void load());
    const doneP = listen('parakeet-model-download-complete', () => void load());
    return () => { done.then((fn) => fn()); doneP.then((fn) => fn()); };
  }, [load]);

  // What the route switched to during this recording
  useEffect(() => {
    if (!isRecording) { setRouted(null); return; }
    let unlisten: (() => void) | undefined;
    listen<RoutedLanguageEvent>('transcription-language-routed', (e) => setRouted(e.payload)).then((fn) => { unlisten = fn; });
    return () => { if (unlisten) unlisten(); };
  }, [isRecording]);

  const currentKey = `${transcriptModelConfig.provider}:${transcriptModelConfig.model}`;
  const current = options.find((o) => o.key === currentKey);
  const localOnly = transcriptModelConfig.provider === 'localWhisper' || transcriptModelConfig.provider === 'parakeet';

  const change = async (key: string) => {
    const opt = options.find((o) => o.key === key);
    if (!opt) return;
    setSaving(true);
    try {
      await invoke('api_save_transcript_config', { provider: opt.provider, model: opt.model, apiKey: null });
      setTranscriptModelConfig((prev) => ({ ...prev, provider: opt.provider, model: opt.model }));
      toast.success(`Transcription model: ${opt.label}`);
    } catch (err) {
      toast.error('Could not change the model', { description: err instanceof Error ? err.message : String(err) });
    } finally {
      setSaving(false);
    }
  };

  const routedLabel = routed
    ? (() => {
        const lang = LANGUAGES.find((l) => l.code === routed.language)?.name ?? routed.language.toUpperCase();
        const opt = options.find((o) => o.provider === (routed.provider === 'localWhisper' ? 'localWhisper' : 'parakeet') && o.model === routed.model);
        return `${lang} detected → ${opt?.label ?? routed.model}`;
      })()
    : null;

  if (!localOnly) {
    return (
      <div className="flex items-center gap-1.5 rounded-full bg-white/90 px-3 py-1 text-xs text-gray-600 shadow">
        <Cpu className="h-3.5 w-3.5 text-gray-400" />
        <span>Transcription: {transcriptModelConfig.provider} · {transcriptModelConfig.model}</span>
      </div>
    );
  }

  return (
    <div className="flex items-center gap-1.5 rounded-full bg-white/90 px-3 py-1 text-xs text-gray-600 shadow" title={isRecording ? 'The model can be changed between recordings' : 'Applies to the next recording'}>
      <Cpu className="h-3.5 w-3.5 text-gray-400" />
      {routedLabel ? (
        <span className="text-gray-800" dir="auto">{routedLabel}</span>
      ) : (
        <>
          <span className="text-gray-500">Model</span>
          <select
            aria-label="Transcription model"
            className="max-w-[260px] cursor-pointer bg-transparent text-xs text-gray-800 focus:outline-none disabled:cursor-not-allowed disabled:text-gray-500"
            value={current ? current.key : ''}
            disabled={isRecording || saving || options.length === 0}
            onChange={(e) => change(e.target.value)}
          >
            {!current && <option value="">{transcriptModelConfig.model} (not downloaded)</option>}
            {options.map((o) => (
              <option key={o.key} value={o.key}>{o.label}</option>
            ))}
          </select>
        </>
      )}
    </div>
  );
}
