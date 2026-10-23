'use client';

import { useCallback, useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { toast } from 'sonner';
import { Plus, Trash2, Loader2 } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Switch } from '@/components/ui/switch';
import { LANGUAGES } from '@/constants/languages';
import { LanguageRoute, LanguageRoutesAPI, emptyRoute } from '@/lib/language-routes';
import type { ModelInfo as WhisperModelInfo } from '@/lib/whisper';
import type { BuiltInModelInfo } from '@/lib/builtin-ai';

interface SttOption {
  provider: 'localWhisper' | 'parakeet';
  model: string;
  label: string;
  languages: string[];
}

interface SummaryOption {
  provider: string;
  model: string;
  label: string;
}

/** Whisper models the routes refer to that are not on disk yet (name → true). */
type MissingWhisper = Record<string, boolean>;

interface ParakeetRawModel {
  name: string;
  status: unknown;
  display_name?: string;
  languages?: string[];
}

const GLOBAL = '__global__';

function keyOf(provider: string | null, model: string | null): string {
  return provider && model ? `${provider}::${model}` : GLOBAL;
}

function splitKey(key: string): { provider: string | null; model: string | null } {
  if (key === GLOBAL) return { provider: null, model: null };
  const idx = key.indexOf('::');
  return { provider: key.slice(0, idx), model: key.slice(idx + 2) };
}

const selectClass =
  'w-full rounded-md border border-gray-200 bg-white px-2 py-1.5 text-sm text-gray-900 focus:outline-none focus:ring-2 focus:ring-blue-300';

/**
 * Settings panel for per-language model routing: for each language pick the
 * transcription model and the summary model, and whether summaries are written
 * directly in that language.
 */
export function LanguageRoutesSettings() {
  const [routes, setRoutes] = useState<LanguageRoute[]>([]);
  const [sttOptions, setSttOptions] = useState<SttOption[]>([]);
  const [summaryOptions, setSummaryOptions] = useState<SummaryOption[]>([]);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState<string | null>(null);
  const [newLanguage, setNewLanguage] = useState('he');
  const [missingWhisper, setMissingWhisper] = useState<MissingWhisper>({});
  const [missingParakeet, setMissingParakeet] = useState<MissingWhisper>({});
  const [downloading, setDownloading] = useState<Record<string, number>>({});

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const [routeList, whisper, parakeet, builtin] = await Promise.all([
        LanguageRoutesAPI.list(),
        invoke<WhisperModelInfo[]>('whisper_get_available_models').catch(() => [] as WhisperModelInfo[]),
        invoke<ParakeetRawModel[]>('parakeet_get_available_models').catch(() => [] as ParakeetRawModel[]),
        invoke<BuiltInModelInfo[]>('builtin_ai_list_models').catch(() => [] as BuiltInModelInfo[]),
      ]);
      setRoutes(routeList);

      const stt: SttOption[] = [
        ...whisper.map((m) => ({
          provider: 'localWhisper' as const,
          model: m.name,
          label: `Whisper: ${m.display_name && m.display_name !== m.name ? m.display_name : m.name}${m.status === 'Available' ? '' : ' (not downloaded)'}`,
          languages: m.languages ?? [],
        })),
        ...parakeet.map((m) => ({
          provider: 'parakeet' as const,
          model: m.name,
          label: `${m.display_name && m.display_name !== m.name ? m.display_name : `Parakeet: ${m.name}`}${m.status === 'Available' ? '' : ' (not downloaded)'}`,
          languages: m.languages && m.languages.length > 0 ? m.languages : ['en'],
        })),
      ];
      setSttOptions(stt);
      const missing: MissingWhisper = {};
      for (const m of whisper) {
        if (m.status !== 'Available') missing[m.name] = true;
      }
      setMissingWhisper(missing);
      const missingP: MissingWhisper = {};
      for (const m of parakeet) {
        if (m.status !== 'Available') missingP[m.name] = true;
      }
      setMissingParakeet(missingP);

      setSummaryOptions(
        builtin.map((m) => ({
          provider: 'builtin-ai',
          model: m.name,
          label: `Built-in: ${m.display_name || m.name}${m.status.type === 'available' ? '' : ' (not downloaded)'}`,
        }))
      );
    } catch (err) {
      console.error('Failed to load language routes:', err);
      toast.error('Failed to load language routes', {
        description: err instanceof Error ? err.message : String(err),
      });
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  // Progress of whisper downloads started from this panel
  useEffect(() => {
    const unlistenProgress = listen<{ modelName: string; progress: number }>('model-download-progress', (e) => {
      setDownloading((d) => (e.payload.modelName in d ? { ...d, [e.payload.modelName]: e.payload.progress } : d));
    });
    const unlistenDone = listen<{ modelName: string }>('model-download-complete', (e) => {
      setDownloading((d) => {
        const next = { ...d };
        delete next[e.payload.modelName];
        return next;
      });
      void load();
    });
    const unlistenParakeet = listen<{ modelName: string; progress: number; status?: string }>('parakeet-model-download-progress', (e) => {
      setDownloading((d) => (e.payload.modelName in d ? { ...d, [e.payload.modelName]: Math.round(e.payload.progress) } : d));
    });
    const unlistenError = listen<{ modelName: string; error: string }>('model-download-error', (e) => {
      setDownloading((d) => {
        const next = { ...d };
        delete next[e.payload.modelName];
        return next;
      });
      toast.error('Model download failed', { description: e.payload.error });
    });
    return () => {
      unlistenProgress.then((fn) => fn());
      unlistenDone.then((fn) => fn());
      unlistenParakeet.then((fn) => fn());
      unlistenError.then((fn) => fn());
    };
  }, [load]);

  const downloadParakeet = async (modelName: string) => {
    setDownloading((d) => ({ ...d, [modelName]: 0 }));
    try {
      await invoke('parakeet_init');
      await invoke('parakeet_download_model', { modelName });
      setDownloading((d) => {
        const next = { ...d };
        delete next[modelName];
        return next;
      });
      void load();
    } catch (err) {
      setDownloading((d) => {
        const next = { ...d };
        delete next[modelName];
        return next;
      });
      toast.error('Model download failed', { description: err instanceof Error ? err.message : String(err) });
    }
  };

  const downloadWhisper = async (modelName: string) => {
    setDownloading((d) => ({ ...d, [modelName]: 0 }));
    try {
      await invoke('whisper_init');
      await invoke('whisper_download_model', { modelName });
    } catch (err) {
      setDownloading((d) => {
        const next = { ...d };
        delete next[modelName];
        return next;
      });
      toast.error('Model download failed', { description: err instanceof Error ? err.message : String(err) });
    }
  };

  const languageName = (code: string) =>
    LANGUAGES.find((l) => l.code === code)?.name ?? code.toUpperCase();

  const availableLanguages = useMemo(
    () =>
      LANGUAGES.filter(
        (l) => l.code !== 'auto' && l.code !== 'auto-translate' && !routes.some((r) => r.language === l.code)
      ),
    [routes]
  );

  // Suggest the model that declares this language (e.g. ivrit.ai for Hebrew)
  const suggestedStt = (language: string): SttOption | undefined =>
    sttOptions.find((o) => o.languages.some((l) => l === language));

  const save = async (route: LanguageRoute) => {
    setSaving(route.language);
    try {
      const saved = await LanguageRoutesAPI.upsert(route);
      setRoutes((prev) => {
        const others = prev.filter((r) => r.language !== saved.language);
        return [...others, saved].sort((a, b) => a.language.localeCompare(b.language));
      });
    } catch (err) {
      toast.error(`Could not save route for ${languageName(route.language)}`, {
        description: err instanceof Error ? err.message : String(err),
      });
    } finally {
      setSaving(null);
    }
  };

  const addRoute = async () => {
    if (!newLanguage) return;
    const route = emptyRoute(newLanguage);
    const suggestion = suggestedStt(newLanguage);
    if (suggestion) {
      route.stt_provider = suggestion.provider;
      route.stt_model = suggestion.model;
    }
    await save(route);
    const next = availableLanguages.find((l) => l.code !== newLanguage);
    setNewLanguage(next?.code ?? '');
    toast.success(`${languageName(newLanguage)} route added`, {
      description: suggestion
        ? `Transcription set to ${suggestion.label}.`
        : 'Pick the models to use for this language.',
    });
  };

  const removeRoute = async (language: string) => {
    try {
      await LanguageRoutesAPI.remove(language);
      setRoutes((prev) => prev.filter((r) => r.language !== language));
    } catch (err) {
      toast.error('Could not remove route', {
        description: err instanceof Error ? err.message : String(err),
      });
    }
  };

  if (loading) {
    return (
      <div className="flex items-center gap-2 text-sm text-gray-500">
        <Loader2 className="h-4 w-4 animate-spin" /> Loading language routes…
      </div>
    );
  }

  return (
    <div className="space-y-4">
      <p className="text-sm text-gray-600">
        Choose the best models per language. With the transcription language set to <b>Auto Detect</b>, the
        first seconds of speech are identified and the matching models are used for the rest of the meeting.
        Summaries for a routed language are written directly in that language.
      </p>

      {routes.length === 0 && (
        <p className="rounded-md border border-dashed border-gray-300 p-3 text-sm text-gray-500">
          No language routes yet. Add Hebrew to use the ivrit.ai transcription model and Hebrew summaries.
        </p>
      )}

      <div className="space-y-3">
        {routes.map((route) => {
          const sttKey = keyOf(route.stt_provider, route.stt_model);
          const summaryKey = keyOf(route.summary_provider, route.summary_model);
          const sttMissing =
            sttKey !== GLOBAL && !sttOptions.some((o) => keyOf(o.provider, o.model) === sttKey);
          const summaryMissing =
            summaryKey !== GLOBAL && !summaryOptions.some((o) => keyOf(o.provider, o.model) === summaryKey);
          const busy = saving === route.language;

          return (
            <div
              key={route.language}
              className={`rounded-lg border p-3 ${route.enabled ? 'border-gray-200 bg-white' : 'border-gray-200 bg-gray-50 opacity-75'}`}
            >
              <div className="mb-3 flex items-center justify-between gap-2">
                <div className="flex items-center gap-2">
                  <span className="font-semibold text-gray-900">{languageName(route.language)}</span>
                  <span className="rounded bg-gray-100 px-1.5 py-0.5 text-xs text-gray-600" dir="ltr">
                    {route.language}
                  </span>
                  {busy && <Loader2 className="h-3.5 w-3.5 animate-spin text-gray-400" />}
                </div>
                <div className="flex items-center gap-3">
                  <label className="flex items-center gap-2 text-xs text-gray-600">
                    Enabled
                    <Switch
                      checked={route.enabled}
                      onCheckedChange={(enabled) => save({ ...route, enabled })}
                    />
                  </label>
                  <button
                    type="button"
                    className="p-1 text-gray-400 hover:text-red-600"
                    title="Remove route"
                    onClick={() => removeRoute(route.language)}
                  >
                    <Trash2 className="h-4 w-4" />
                  </button>
                </div>
              </div>

              <div className="grid gap-3 sm:grid-cols-2">
                <label className="space-y-1 text-xs text-gray-600">
                  <span>Transcription model</span>
                  <select
                    className={selectClass}
                    value={sttKey}
                    onChange={(e) => {
                      const { provider, model } = splitKey(e.target.value);
                      save({ ...route, stt_provider: provider, stt_model: model });
                    }}
                  >
                    <option value={GLOBAL}>Use global setting</option>
                    {sttMissing && <option value={sttKey}>{route.stt_model} (not installed)</option>}
                    {sttOptions.map((o) => (
                      <option key={keyOf(o.provider, o.model)} value={keyOf(o.provider, o.model)}>
                        {o.label}
                        {o.languages.includes(route.language) ? ' ★' : ''}
                      </option>
                    ))}
                  </select>
                  {route.stt_provider === 'parakeet' && route.stt_model && missingParakeet[route.stt_model] && (
                    <span className="flex items-center gap-2 pt-1">
                      {route.stt_model in downloading ? (
                        <>
                          <Loader2 className="h-3.5 w-3.5 animate-spin text-gray-400" />
                          <span className="text-amber-700">Downloading… {downloading[route.stt_model]}%</span>
                        </>
                      ) : (
                        <>
                          <span className="text-amber-700">Model not downloaded; {languageName(route.language)} will fall back to the global model.</span>
                          <button
                            type="button"
                            className="rounded border border-gray-300 bg-white px-2 py-0.5 text-xs font-medium text-gray-800 hover:bg-gray-50"
                            onClick={() => downloadParakeet(route.stt_model as string)}
                          >
                            Download
                          </button>
                        </>
                      )}
                    </span>
                  )}
                  {route.stt_provider === 'localWhisper' && route.stt_model && missingWhisper[route.stt_model] && (
                    <span className="flex items-center gap-2 pt-1">
                      {route.stt_model in downloading ? (
                        <>
                          <Loader2 className="h-3.5 w-3.5 animate-spin text-gray-400" />
                          <span className="text-amber-700">Downloading… {downloading[route.stt_model]}%</span>
                        </>
                      ) : (
                        <>
                          <span className="text-amber-700">Model not downloaded; {languageName(route.language)} will fall back to the global model.</span>
                          <button
                            type="button"
                            className="rounded border border-gray-300 bg-white px-2 py-0.5 text-xs font-medium text-gray-800 hover:bg-gray-50"
                            onClick={() => downloadWhisper(route.stt_model as string)}
                          >
                            Download
                          </button>
                        </>
                      )}
                    </span>
                  )}
                </label>

                <label className="space-y-1 text-xs text-gray-600">
                  <span>Summary model</span>
                  <select
                    className={selectClass}
                    value={summaryKey}
                    onChange={(e) => {
                      const { provider, model } = splitKey(e.target.value);
                      save({ ...route, summary_provider: provider, summary_model: model });
                    }}
                  >
                    <option value={GLOBAL}>Use global setting</option>
                    {summaryMissing && (
                      <option value={summaryKey}>
                        {route.summary_provider}: {route.summary_model} (not installed)
                      </option>
                    )}
                    {summaryOptions.map((o) => (
                      <option key={keyOf(o.provider, o.model)} value={keyOf(o.provider, o.model)}>
                        {o.label}
                      </option>
                    ))}
                  </select>
                </label>
              </div>

              <label className="mt-3 flex items-center justify-between gap-3 text-xs text-gray-600">
                <span>
                  Write summaries directly in {languageName(route.language)}
                  <span className="block text-gray-400">
                    Off: summarise in English first, then translate (upstream behaviour).
                  </span>
                </span>
                <Switch
                  checked={route.summary_mode === 'direct'}
                  onCheckedChange={(direct) =>
                    save({ ...route, summary_mode: direct ? 'direct' : 'translate' })
                  }
                />
              </label>
            </div>
          );
        })}
      </div>

      <div className="flex items-center gap-2">
        <select
          className={`${selectClass} max-w-xs`}
          value={newLanguage}
          onChange={(e) => setNewLanguage(e.target.value)}
        >
          {availableLanguages.length === 0 && <option value="">All languages routed</option>}
          {availableLanguages.map((l) => (
            <option key={l.code} value={l.code}>
              {l.name}
            </option>
          ))}
        </select>
        <Button type="button" variant="outline" size="sm" onClick={addRoute} disabled={!newLanguage}>
          <Plus className="mr-1 h-4 w-4" /> Add language
        </Button>
      </div>
    </div>
  );
}
