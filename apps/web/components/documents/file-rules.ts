import { ApiError } from '@/lib/api';
import { formatBytes } from '@/lib/format';

/** The files the API reads (ADR-095), offered by the file picker. */
export const ACCEPTED = '.pdf,.docx,.md,.markdown,.txt';

/**
 * Refuses, before it is sent, a file beyond the largest the API takes,
 * which it gives with the list (ADR-097): at a deposit as at a new
 * version (ADR-151).
 */
export function refuseBeyond(file: File, maxFileBytes: number): void {
  if (file.size > maxFileBytes) {
    throw new ApiError(
      413,
      'too_large',
      `Ce fichier dépasse ${formatBytes(maxFileBytes)} : il n’est pas envoyé.`,
    );
  }
}
