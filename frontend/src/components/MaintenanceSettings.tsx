'use client';

import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { AlertTriangle, Loader2, RefreshCw, Trash2 } from 'lucide-react';

interface DataLocation {
  path: string;
  kind: 'data' | 'models' | 'webview' | 'cache' | 'state' | string;
  exists: boolean;
  size_bytes: number;
}

interface DataReport {
  locations: DataLocation[];
  app_path: string | null;
  total_bytes: number;
  models_bytes: number;
}

const KIND_LABEL: Record<string, string> = {
  data: 'Meetings, settings, voices',
  models: 'Downloaded models',
  webview: 'Web view storage',
  cache: 'Caches',
  state: 'Window state',
};

function formatBytes(n: number): string {
  if (n >= 1 << 30) return `${(n / (1 << 30)).toFixed(2)} GB`;
  if (n >= 1 << 20) return `${(n / (1 << 20)).toFixed(1)} MB`;
  if (n >= 1 << 10) return `${(n / (1 << 10)).toFixed(0)} KB`;
  return `${n} B`;
}

type Pending = null | 'reset-keep' | 'reset-all' | 'uninstall';

/**
 * Storage inventory plus the three destructive actions: reset keeping models,
 * reset everything, uninstall. Each needs a typed confirmation.
 */
export function MaintenanceSettings() {
  const [report, setReport] = useState<DataReport | null>(null);
  const [pending, setPending] = useState<Pending>(null);
  const [confirmText, setConfirmText] = useState('');
  const [busy, setBusy] = useState(false);

  const load = async () => {
    try {
      setReport(await invoke<DataReport>('maintenance_data_report'));
    } catch (err) {
      console.error('maintenance_data_report failed', err);
    }
  };

  useEffect(() => {
    void load();
  }, []);

  const run = async () => {
    if (!pending || confirmText.trim().toUpperCase() !== 'DELETE') return;
    setBusy(true);
    try {
      if (pending === 'uninstall') {
        const message = await invoke<string>('maintenance_uninstall');
        toast.success('Tamlil is uninstalling', { description: message, duration: 10000 });
      } else {
        toast.success('Resetting Tamlil', { description: 'The app will close. Start it again for a fresh setup.' });
        await invoke('maintenance_reset', { keepModels: pending === 'reset-keep' });
      }
    } catch (err) {
      setBusy(false);
      toast.error('Could not complete', { description: err instanceof Error ? err.message : String(err) });
    }
  };

  const describe: Record<Exclude<Pending, null>, { title: string; body: string }> = {
    'reset-keep': {
      title: 'Reset app data, keep models',
      body: 'Deletes all meetings, transcripts, summaries, settings, remembered voices and caches. Downloaded models stay, so setup is quick. Tamlil closes and starts fresh next time.',
    },
    'reset-all': {
      title: 'Reset everything',
      body: `Deletes all meetings, settings, remembered voices, caches and every downloaded model (${report ? formatBytes(report.models_bytes) : '…'}). Tamlil closes and starts fresh next time.`,
    },
    uninstall: {
      title: 'Uninstall Tamlil',
      body: 'Deletes all data and models, then removes the application itself (moved to the Trash on macOS). Nothing is left on this computer.',
    },
  };

  return (
    <div className="mt-6 rounded-lg border border-red-200 bg-white p-4">
      <div className="flex items-center justify-between">
        <h3 className="text-sm font-semibold text-gray-900">Storage, reset and uninstall</h3>
        <button type="button" className="inline-flex items-center gap-1 text-xs text-gray-500 hover:text-gray-800" onClick={load}>
          <RefreshCw className="h-3.5 w-3.5" /> Refresh
        </button>
      </div>
      <p className="mt-1 text-xs text-gray-600">
        Everything Tamlil stores on this computer. Dragging the app to the Trash leaves all of it behind; use the
        buttons below instead.
      </p>

      {report && (
        <table className="mt-3 w-full text-xs">
          <tbody>
            {report.locations.filter((l) => l.exists).map((l) => (
              <tr key={l.path} className="border-t border-gray-100">
                <td className="py-1 pr-2 text-gray-500 whitespace-nowrap">{KIND_LABEL[l.kind] ?? l.kind}</td>
                <td className="py-1 pr-2 font-mono text-[11px] text-gray-700 break-all">{l.path}</td>
                <td className="py-1 text-right text-gray-700 whitespace-nowrap">{formatBytes(l.size_bytes)}</td>
              </tr>
            ))}
            <tr className="border-t border-gray-200 font-semibold">
              <td className="py-1 text-gray-700">Total</td>
              <td />
              <td className="py-1 text-right text-gray-900">{formatBytes(report.total_bytes)}</td>
            </tr>
          </tbody>
        </table>
      )}

      {!pending ? (
        <div className="mt-3 flex flex-wrap gap-2">
          <button type="button" className="rounded-md border border-gray-300 px-3 py-1.5 text-xs font-medium text-gray-800 hover:bg-gray-50" onClick={() => { setConfirmText(''); setPending('reset-keep'); }}>
            Reset app data (keep models)
          </button>
          <button type="button" className="rounded-md border border-gray-300 px-3 py-1.5 text-xs font-medium text-gray-800 hover:bg-gray-50" onClick={() => { setConfirmText(''); setPending('reset-all'); }}>
            Reset everything
          </button>
          <button type="button" className="inline-flex items-center gap-1 rounded-md border border-red-300 bg-red-50 px-3 py-1.5 text-xs font-medium text-red-700 hover:bg-red-100" onClick={() => { setConfirmText(''); setPending('uninstall'); }}>
            <Trash2 className="h-3.5 w-3.5" /> Uninstall Tamlil…
          </button>
        </div>
      ) : (
        <div className="mt-3 rounded-md border border-red-200 bg-red-50 p-3">
          <div className="flex items-start gap-2">
            <AlertTriangle className="mt-0.5 h-4 w-4 flex-shrink-0 text-red-600" />
            <div className="min-w-0">
              <div className="text-sm font-semibold text-red-800">{describe[pending].title}</div>
              <p className="mt-1 text-xs text-red-800">{describe[pending].body}</p>
              <p className="mt-2 text-xs text-red-800">
                Type <span className="font-mono font-semibold">DELETE</span> to confirm. This cannot be undone.
              </p>
              <div className="mt-2 flex items-center gap-2">
                <input
                  autoFocus
                  value={confirmText}
                  onChange={(e) => setConfirmText(e.target.value)}
                  className="h-8 w-32 rounded border border-red-300 px-2 text-sm"
                  placeholder="DELETE"
                  aria-label="Type DELETE to confirm"
                />
                <button
                  type="button"
                  disabled={busy || confirmText.trim().toUpperCase() !== 'DELETE'}
                  onClick={run}
                  className="inline-flex h-8 items-center gap-1 rounded-md bg-red-600 px-3 text-xs font-semibold text-white hover:bg-red-700 disabled:cursor-not-allowed disabled:opacity-50"
                >
                  {busy ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : null}
                  {describe[pending].title}
                </button>
                <button type="button" disabled={busy} className="h-8 rounded-md px-3 text-xs text-gray-700 hover:bg-white" onClick={() => setPending(null)}>
                  Cancel
                </button>
              </div>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
