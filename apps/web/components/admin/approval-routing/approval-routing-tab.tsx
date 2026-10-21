'use client';

import { Loaded } from '@/components/ui';
import { useApiData, type ApprovalRoutingView } from '@/lib/api';
import { ApprovalRoutingForm } from './approval-routing-form';

/** Which actions a manager validates, rather than whom the agent acts for (ADR-059). */
export function ApprovalRoutingTab() {
  const state = useApiData<ApprovalRoutingView>('/admin/approval-routing');
  return (
    <Loaded state={state}>
      {(data) => <ApprovalRoutingForm data={data} onSaved={state.reload} />}
    </Loaded>
  );
}
