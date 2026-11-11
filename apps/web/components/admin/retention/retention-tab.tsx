'use client';

import { Loaded } from '@/components/ui';
import { useApiData, type RetentionView } from '@/lib/api';
import { RetentionForm } from './retention-form';

/** How long the company keeps its idle conversations, its episodes and the
 * personal memories of its accounts gone (ADR-195, ADR-196): what it chose,
 * what the purge applies now, and when a shortening applies. */
export function RetentionTab() {
  const state = useApiData<RetentionView>('/admin/retention');
  return (
    <Loaded state={state}>{(data) => <RetentionForm data={data} onSaved={state.reload} />}</Loaded>
  );
}
