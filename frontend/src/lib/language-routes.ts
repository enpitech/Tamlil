// Per-language model routing. Mirrors src-tauri/src/language_routing.

import { invoke } from '@tauri-apps/api/core';

export type SummaryMode = 'direct' | 'translate';

export interface LanguageRoute {
  /** ISO-639-1 code, lower case. */
  language: string;
  /** 'localWhisper' | 'parakeet' | null (keep the global transcription setting). */
  stt_provider: string | null;
  stt_model: string | null;
  /** Summary provider id ('builtin-ai', 'ollama', ...) or null (keep the global setting). */
  summary_provider: string | null;
  summary_model: string | null;
  summary_mode: SummaryMode;
  enabled: boolean;
}

export interface RoutedLanguageEvent {
  language: string;
  confidence: number;
  provider: string;
  model: string;
  switched: boolean;
}

export function emptyRoute(language: string): LanguageRoute {
  return {
    language,
    stt_provider: null,
    stt_model: null,
    summary_provider: null,
    summary_model: null,
    summary_mode: 'direct',
    enabled: true,
  };
}

export class LanguageRoutesAPI {
  static list(): Promise<LanguageRoute[]> {
    return invoke<LanguageRoute[]>('language_routes_list');
  }

  static upsert(route: LanguageRoute): Promise<LanguageRoute> {
    return invoke<LanguageRoute>('language_routes_upsert', { route });
  }

  static remove(language: string): Promise<void> {
    return invoke<void>('language_routes_delete', { language });
  }

  static detectedLanguage(): Promise<string | null> {
    return invoke<string | null>('language_routes_detected_language');
  }
}
