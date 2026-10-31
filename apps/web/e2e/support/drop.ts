// A file dropped on the page, as a browser does it: a drag over, then a
// drop, carrying the file (ADR-151).
import type { Locator } from '@playwright/test';

/** Drops on `zone` the file `name` holding `text`. */
export async function drop(zone: Locator, name: string, text: string) {
  const transfer = await zone.page().evaluateHandle(
    ({ name, text }) => {
      const dropped = new DataTransfer();
      dropped.items.add(new File([text], name, { type: 'text/markdown' }));
      return dropped;
    },
    { name, text },
  );
  await zone.dispatchEvent('dragover', { dataTransfer: transfer });
  await zone.dispatchEvent('drop', { dataTransfer: transfer });
}
