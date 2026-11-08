import type { DownloadedFile } from '@/lib/api';

/** How long a saved file's address lives: the browser has taken it by then. */
const KEPT_MS = 60_000;

/** Hands `file` to the browser to save, under its name. */
export function saveFile({ name, blob }: DownloadedFile): void {
  const address = URL.createObjectURL(blob);
  const link = document.createElement('a');
  link.href = address;
  link.download = name;
  document.body.append(link);
  link.click();
  link.remove();
  setTimeout(() => URL.revokeObjectURL(address), KEPT_MS);
}
