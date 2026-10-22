'use client';

import { useState } from 'react';
import { toast } from 'sonner';
import { Link2, FolderOpen, Loader2 } from 'lucide-react';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { cn } from '@/lib/utils';
import {
  EntryOverrides,
  ModelKind,
  ModelRegistryAPI,
  RegistryEntry,
  parseLanguageList,
} from '@/lib/model-registry';

interface AddModelDialogProps {
  kind: ModelKind;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** Called with the new entry after it was registered (and, for files, copied). */
  onAdded: (entry: RegistryEntry) => void;
}

type Source = 'url' | 'file';

const KIND_COPY: Record<ModelKind, { title: string; ext: string; urlHint: string }> = {
  whisper_ggml: {
    title: 'Add transcription model',
    ext: '.bin',
    urlHint: 'https://huggingface.co/ivrit-ai/whisper-large-v3-turbo-ggml/resolve/main/ggml-model.bin',
  },
  gguf: {
    title: 'Add summary model',
    ext: '.gguf',
    urlHint: 'https://huggingface.co/.../resolve/main/model-Q4_K_M.gguf',
  },
};

export function AddModelDialog({ kind, open, onOpenChange, onAdded }: AddModelDialogProps) {
  const copy = KIND_COPY[kind];
  const [source, setSource] = useState<Source>('url');
  const [url, setUrl] = useState('');
  const [filePath, setFilePath] = useState<string | null>(null);
  const [displayName, setDisplayName] = useState('');
  const [languages, setLanguages] = useState('');
  const [contextSize, setContextSize] = useState('');
  const [busy, setBusy] = useState(false);

  const reset = () => {
    setSource('url');
    setUrl('');
    setFilePath(null);
    setDisplayName('');
    setLanguages('');
    setContextSize('');
    setBusy(false);
  };

  const close = () => {
    reset();
    onOpenChange(false);
  };

  const pickFile = async () => {
    try {
      const picked = await ModelRegistryAPI.pickFile(kind);
      if (picked) setFilePath(picked);
    } catch (err) {
      toast.error('Could not open file picker', {
        description: err instanceof Error ? err.message : String(err),
      });
    }
  };

  const buildOverrides = (): EntryOverrides => {
    const overrides: EntryOverrides = {};
    if (displayName.trim()) overrides.display_name = displayName.trim();
    const langs = parseLanguageList(languages);
    if (langs.length) overrides.languages = langs;
    if (kind === 'gguf' && contextSize.trim()) {
      const n = Number.parseInt(contextSize, 10);
      if (Number.isFinite(n) && n > 0) overrides.context_size = n;
    }
    return overrides;
  };

  const submit = async () => {
    const overrides = buildOverrides();
    setBusy(true);
    try {
      let entry: RegistryEntry;
      if (source === 'url') {
        if (!url.trim()) {
          toast.error('Enter a download URL');
          setBusy(false);
          return;
        }
        entry = await ModelRegistryAPI.addFromUrl(kind, url.trim(), overrides);
        toast.success(`${entry.display_name} added`, {
          description: 'Use Download to fetch the file.',
        });
      } else {
        if (!filePath) {
          toast.error('Choose a model file');
          setBusy(false);
          return;
        }
        toast.info('Importing model file…', { description: 'Large files can take a moment to copy.' });
        entry = await ModelRegistryAPI.importLocal(kind, filePath, overrides);
        toast.success(`${entry.display_name} imported`);
      }
      onAdded(entry);
      close();
    } catch (err) {
      toast.error('Could not add model', {
        description: err instanceof Error ? err.message : String(err),
        duration: 6000,
      });
      setBusy(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={(next) => (next ? onOpenChange(true) : close())}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>{copy.title}</DialogTitle>
          <DialogDescription>
            Register any {copy.ext} model from a download link or a file on this computer. It will
            appear next to the built-in models.
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-4">
          <div className="grid grid-cols-2 gap-2">
            <SourceButton
              active={source === 'url'}
              onClick={() => setSource('url')}
              icon={<Link2 className="h-4 w-4" />}
              label="From URL"
            />
            <SourceButton
              active={source === 'file'}
              onClick={() => setSource('file')}
              icon={<FolderOpen className="h-4 w-4" />}
              label="From file"
            />
          </div>

          {source === 'url' ? (
            <div className="space-y-1.5">
              <Label htmlFor="model-url">Download URL</Label>
              <Input
                id="model-url"
                dir="ltr"
                placeholder={copy.urlHint}
                value={url}
                onChange={(e) => setUrl(e.target.value)}
                autoFocus
              />
              <p className="text-xs text-muted-foreground">
                Direct link to a {copy.ext} file. Hugging Face “resolve” links work.
              </p>
            </div>
          ) : (
            <div className="space-y-1.5">
              <Label>Model file</Label>
              <div className="flex items-center gap-2">
                <Button type="button" variant="outline" onClick={pickFile}>
                  Choose file…
                </Button>
                <span className="min-w-0 flex-1 truncate text-sm text-muted-foreground" dir="ltr">
                  {filePath ?? `No ${copy.ext} file selected`}
                </span>
              </div>
              <p className="text-xs text-muted-foreground">
                The file is moved into the app’s models folder.
              </p>
            </div>
          )}

          <div className="grid gap-3 sm:grid-cols-2">
            <div className="space-y-1.5">
              <Label htmlFor="model-display-name">Display name (optional)</Label>
              <Input
                id="model-display-name"
                placeholder="e.g. ivrit.ai Large v3 Turbo"
                value={displayName}
                onChange={(e) => setDisplayName(e.target.value)}
              />
            </div>
            <div className="space-y-1.5">
              <Label htmlFor="model-languages">Languages (optional)</Label>
              <Input
                id="model-languages"
                dir="ltr"
                placeholder="he, en"
                value={languages}
                onChange={(e) => setLanguages(e.target.value)}
              />
              <p className="text-xs text-muted-foreground">
                Language codes this model is best at. Leave empty for multilingual.
              </p>
            </div>
          </div>

          {kind === 'gguf' && (
            <div className="space-y-1.5">
              <Label htmlFor="model-context">Context window (tokens, optional)</Label>
              <Input
                id="model-context"
                dir="ltr"
                inputMode="numeric"
                placeholder="8192"
                value={contextSize}
                onChange={(e) => setContextSize(e.target.value)}
              />
              <p className="text-xs text-muted-foreground">
                The chat format is read from the GGUF file automatically.
              </p>
            </div>
          )}
        </div>

        <DialogFooter>
          <Button type="button" variant="outline" onClick={close} disabled={busy}>
            Cancel
          </Button>
          <Button type="button" onClick={submit} disabled={busy}>
            {busy && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
            {source === 'url' ? 'Add model' : 'Import model'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function SourceButton({
  active,
  onClick,
  icon,
  label,
}: {
  active: boolean;
  onClick: () => void;
  icon: React.ReactNode;
  label: string;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={cn(
        'flex items-center justify-center gap-2 rounded-md border px-3 py-2 text-sm transition-colors',
        active
          ? 'border-gray-900 bg-gray-900 text-white'
          : 'border-gray-200 bg-white text-gray-700 hover:border-gray-300'
      )}
    >
      {icon}
      {label}
    </button>
  );
}
