// Model registry: user-extensible catalog of local models (Whisper GGML for
// transcription, GGUF for the built-in summary engine). Mirrors
// src-tauri/src/model_registry.

import { invoke } from '@tauri-apps/api/core';

export type ModelKind = 'whisper_ggml' | 'gguf';
export type ModelOrigin = 'built_in' | 'user' | 'discovered';

export interface RegistryEntry {
  id: string;
  kind: ModelKind;
  display_name: string;
  file_name: string;
  download_url: string | null;
  size_mb: number | null;
  sha256: string | null;
  languages: string[];
  description: string;
  origin: ModelOrigin;
  accuracy: string | null;
  speed: string | null;
  template: string | null;
  context_size: number | null;
  layer_count: number | null;
  sampling: string | null;
}

export interface EntryOverrides {
  id?: string;
  display_name?: string;
  file_name?: string;
  languages?: string[];
  description?: string;
  size_mb?: number;
  sha256?: string;
  template?: string;
  context_size?: number;
  layer_count?: number;
  sampling?: string;
}

export const ORIGIN_LABELS: Record<ModelOrigin, string> = {
  built_in: 'Built-in',
  user: 'Added by you',
  discovered: 'Found in models folder',
};

export function parseLanguageList(input: string): string[] {
  return input
    .split(/[,\s]+/)
    .map((s) => s.trim().toLowerCase())
    .filter((s) => s.length > 0);
}

export class ModelRegistryAPI {
  static list(kind: ModelKind): Promise<RegistryEntry[]> {
    return invoke<RegistryEntry[]>('registry_list_models', { kind });
  }

  static getModelsDirectory(kind: ModelKind): Promise<string> {
    return invoke<string>('registry_get_models_directory', { kind });
  }

  static addFromUrl(kind: ModelKind, url: string, overrides: EntryOverrides = {}): Promise<RegistryEntry> {
    return invoke<RegistryEntry>('registry_add_model_from_url', { args: { kind, url, overrides } });
  }

  static pickFile(kind: ModelKind): Promise<string | null> {
    return invoke<string | null>('registry_pick_model_file', { kind });
  }

  static importLocal(kind: ModelKind, path: string, overrides: EntryOverrides = {}): Promise<RegistryEntry> {
    return invoke<RegistryEntry>('registry_import_local_model', { args: { kind, path, overrides } });
  }

  static update(kind: ModelKind, id: string, overrides: EntryOverrides): Promise<RegistryEntry> {
    return invoke<RegistryEntry>('registry_update_model', { kind, id, overrides });
  }

  static remove(kind: ModelKind, id: string, deleteFile: boolean): Promise<void> {
    return invoke<void>('registry_remove_model', { kind, id, deleteFile });
  }
}
