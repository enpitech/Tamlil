'use client';

import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { toast } from 'sonner';
import { Download, Loader2, Trash2 } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Switch } from '@/components/ui/switch';
import { SpeakerAvatar } from './SpeakerAvatar';

interface SpeakerIdStatus {
  enabled: boolean;
  model_available: boolean;
  model_path: string | null;
  model_size_mb: number;
}

interface VoiceProfile {
  id: string;
  name: string;
  samples: number;
  updated_at: string;
}

const STORAGE_KEY = 'tamlil.speakerId.enabled';

/** Persisted preference, synced to the backend on startup by ConfigContext-style callers. */
export function readSpeakerIdPreference(): boolean {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    return raw === null ? true : raw === 'true';
  } catch {
    return true;
  }
}

export function SpeakerIdSettings() {
  const [status, setStatus] = useState<SpeakerIdStatus | null>(null);
  const [downloading, setDownloading] = useState(false);
  const [progress, setProgress] = useState(0);
  const [profiles, setProfiles] = useState<VoiceProfile[]>([]);

  const refresh = async () => {
    try {
      const s = await invoke<SpeakerIdStatus>('speaker_id_get_status');
      setStatus(s);
      const list = await invoke<VoiceProfile[]>('speaker_profiles_list');
      setProfiles(list);
    } catch (err) {
      console.error('speaker_id_get_status failed', err);
    }
  };

  const forgetVoice = async (id: string, name: string) => {
    try {
      await invoke('speaker_profiles_delete', { id });
      setProfiles((p) => p.filter((v) => v.id !== id));
      toast.success(`Forgot ${name}'s voice`);
    } catch (err) {
      toast.error('Could not remove voice', { description: err instanceof Error ? err.message : String(err) });
    }
  };

  const forgetAll = async () => {
    try {
      await invoke('speaker_profiles_clear');
      setProfiles([]);
      toast.success('All remembered voices removed');
    } catch (err) {
      toast.error('Could not remove voices', { description: err instanceof Error ? err.message : String(err) });
    }
  };

  useEffect(() => {
    void refresh();
    let unlisten: (() => void) | undefined;
    listen<{ progress: number; done?: boolean }>('speaker-id-download-progress', (e) => {
      setProgress(e.payload.progress);
      if (e.payload.done) {
        setDownloading(false);
        void refresh();
      }
    }).then((fn) => { unlisten = fn; });
    return () => { if (unlisten) unlisten(); };
  }, []);

  const setEnabled = async (enabled: boolean) => {
    try {
      await invoke('speaker_id_set_enabled', { enabled });
      try { localStorage.setItem(STORAGE_KEY, String(enabled)); } catch { /* ignore */ }
      setStatus((s) => (s ? { ...s, enabled } : s));
    } catch (err) {
      toast.error('Could not update speaker identification', {
        description: err instanceof Error ? err.message : String(err),
      });
    }
  };

  const download = async () => {
    setDownloading(true);
    setProgress(0);
    try {
      await invoke('speaker_id_download_model');
      toast.success('Speaker model ready');
    } catch (err) {
      toast.error('Speaker model download failed', {
        description: err instanceof Error ? err.message : String(err),
      });
    } finally {
      setDownloading(false);
      void refresh();
    }
  };

  const remove = async () => {
    try {
      await invoke('speaker_id_delete_model');
      void refresh();
    } catch (err) {
      toast.error('Could not delete the speaker model', {
        description: err instanceof Error ? err.message : String(err),
      });
    }
  };

  if (!status) return null;

  return (
    <div className="rounded-lg border border-gray-200 bg-white p-4 space-y-3">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <div className="flex -space-x-1">
              <SpeakerAvatar index={0} label="You" size={22} />
              <SpeakerAvatar index={1} label="Speaker 1" size={22} />
              <SpeakerAvatar index={2} label="Speaker 2" size={22} />
            </div>
            <h4 className="text-sm font-semibold text-gray-900">Identify speakers</h4>
            <span className="rounded bg-amber-100 px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-wide text-amber-800">beta</span>
          </div>
          <p className="mt-1 text-xs text-gray-600">
            Tells voices apart by their sound and gives each one an avatar in the transcript. Your microphone
            is “You”; voices from the call become Speaker 1, 2, … Runs on this device with a small
            ({status.model_size_mb} MB) model; nothing is stored about voices between meetings.
          </p>
        </div>
        <Switch
          checked={status.enabled}
          onCheckedChange={setEnabled}
          disabled={!status.model_available}
          aria-label="Identify speakers"
        />
      </div>

      {!status.model_available ? (
        <div className="flex items-center gap-3">
          <Button size="sm" variant="outline" onClick={download} disabled={downloading}>
            {downloading ? <Loader2 className="mr-2 h-4 w-4 animate-spin" /> : <Download className="mr-2 h-4 w-4" />}
            {downloading ? `Downloading… ${progress}%` : `Download speaker model (${status.model_size_mb} MB)`}
          </Button>
          {downloading && (
            <div className="h-2 flex-1 overflow-hidden rounded-full bg-gray-200">
              <div className="h-full bg-gray-800 transition-all" style={{ width: `${progress}%` }} />
            </div>
          )}
        </div>
      ) : (
        <div className="flex items-center justify-between text-xs text-gray-500">
          <span>Model installed{status.enabled ? ' · active for new recordings' : ' · switched off'}</span>
          <button type="button" className="inline-flex items-center gap-1 text-gray-400 hover:text-red-600" onClick={remove}>
            <Trash2 className="h-3.5 w-3.5" /> Remove model
          </button>
        </div>
      )}

      <div className="border-t border-gray-100 pt-3">
        <div className="flex items-center justify-between">
          <h5 className="text-xs font-semibold text-gray-800">Remembered voices</h5>
          {profiles.length > 0 && (
            <button type="button" className="text-xs text-gray-400 hover:text-red-600" onClick={forgetAll}>
              Forget all
            </button>
          )}
        </div>
        <p className="mt-1 text-xs text-gray-500">
          Click a speaker’s name in a transcript to name them. Their voice is then saved to a local file on this
          device and recognised in future meetings. Nothing is uploaded.
        </p>
        {profiles.length > 0 && (
          <ul className="mt-2 divide-y divide-gray-100 rounded-md border border-gray-100">
            {profiles.map((p, i) => (
              <li key={p.id} className="flex items-center justify-between gap-2 px-2 py-1.5 text-xs">
                <span className="flex items-center gap-2 min-w-0">
                  <SpeakerAvatar index={i + 1} label={p.name} size={20} />
                  <span className="truncate font-medium text-gray-800" dir="auto">{p.name}</span>
                  <span className="text-gray-400">{p.samples} clip{p.samples === 1 ? '' : 's'}</span>
                </span>
                <button
                  type="button"
                  className="text-gray-400 hover:text-red-600"
                  title="Forget this voice"
                  onClick={() => forgetVoice(p.id, p.name)}
                >
                  <Trash2 className="h-3.5 w-3.5" />
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}
