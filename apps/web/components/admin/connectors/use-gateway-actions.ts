'use client';

import { useState } from 'react';
import { api, useAction, type GatewayUpdate, type GatewayView } from '@/lib/api';

/** Which of a gateway card's buttons was pressed last. */
export type GatewayButton = 'check' | 'toggle' | 'remove';

/**
 * What a gateway's card does to it (ADR-126, ADR-134): switch it on or off,
 * check it with its service, delete it once `question` is confirmed; the
 * button pressed shows the wait, the others are held meanwhile.
 */
export function useGatewayActions(gateway: GatewayView, onChanged: () => void) {
  const [pressed, setPressed] = useState<GatewayButton | null>(null);
  const action = useAction();
  const path = `/admin/gateways/${gateway.id}`;

  async function run(button: GatewayButton, call: () => Promise<unknown>) {
    setPressed(button);
    if (await action.run(call)) onChanged();
  }

  return {
    action,
    /** Whether `button` waits for its answer. */
    loading: (button: GatewayButton) => action.pending && pressed === button,
    /** Whether `button` is held while another one waits. */
    held: (button: GatewayButton) => action.pending && pressed !== button,
    toggle: () =>
      run('toggle', () => api.patch(path, { enabled: !gateway.enabled } satisfies GatewayUpdate)),
    check: () => run('check', () => api.post(`${path}/check`)),
    remove: (question: string) => {
      if (!window.confirm(question)) return Promise.resolve();
      return run('remove', () => api.delete(path));
    },
  };
}
