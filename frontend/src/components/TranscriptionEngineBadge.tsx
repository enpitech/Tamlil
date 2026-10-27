'use client';

import { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { toast } from 'sonner';
import { Cpu } from 'lucide-react';
import { useConfig } from '@/contexts/ConfigContext';
import type { RoutedLanguageEvent } from '@/lib/language-routes';
import { LANGUAGES } from '@/constants/languages';

const QUICK_LANGUAGES = [
  { code: 'he', name: 'Hebrew' },
  { code: 'en', name: 'English' },
  { code: 'auto', name: 'Auto detect' },
];

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
  const { transcriptModelConfig, setTranscriptModelConfig, selectedLanguage, setSelectedLanguage } = useConfig();
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
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    listen<RoutedLanguageEvent>('transcription-language-routed', (e) => setRouted(e.payload)).then((fn) => {
      if (cancelled) fn(); else unlisten = fn;
    });
    return () => { cancelled = true; if (unlisten) unlisten(); };
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
      <div className="flex items-center gap-1.5 text-xs text-gray-600 whitespace-nowrap">
        <Cpu className="h-3.5 w-3.5 text-gray-400" />
        <span>Transcription: {transcriptModelConfig.provider} · {transcriptModelConfig.model}</span>
      </div>
    );
  }

  return (
    <div className="flex items-center gap-1.5 text-xs text-gray-600 whitespace-nowrap" title={isRecording ? 'The model can be changed between recordings' : 'Applies to the next recording'}>
      <Cpu className="h-3.5 w-3.5 text-gray-400" />
      <select
        aria-label="Transcription language"
        className="cursor-pointer bg-transparent text-xs text-gray-800 focus:outline-none disabled:cursor-not-allowed disabled:text-gray-500"
        value={QUICK_LANGUAGES.some((l) => l.code === selectedLanguage) ? selectedLanguage : ''}
        disabled={isRecording}
        onChange={(e) => setSelectedLanguage(e.target.value)}
      >
        {!QUICK_LANGUAGES.some((l) => l.code === selectedLanguage) && (
          <option value="">{LANGUAGES.find((l) => l.code === selectedLanguage)?.name ?? selectedLanguage}</option>
        )}
        {QUICK_LANGUAGES.map((l) => (
          <option key={l.code} value={l.code}>{l.name}</option>
        ))}
      </select>
      <span className="text-gray-300" aria-hidden="true">·</span>
      {routedLabel ? (
        <span className="text-gray-800" dir="auto">{routedLabel}</span>
      ) : (
        <>
          <span className="text-gray-500">Model</span>
          <select
            aria-label="Transcription model"
            className="max-w-[220px] truncate cursor-pointer bg-transparent text-xs text-gray-800 focus:outline-none disabled:cursor-not-allowed disabled:text-gray-500"
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
