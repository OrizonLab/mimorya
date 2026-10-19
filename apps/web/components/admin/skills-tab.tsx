'use client';

import { Badge, Card, EmptyState, Loaded } from '@/components/ui';
import { useApiData, type SkillList } from '@/lib/api';

export function SkillsTab() {
  const state = useApiData<SkillList>('/admin/skills');
  return (
    <Loaded state={state}>
      {(data) =>
        data.skills.length === 0 ? (
          <EmptyState title="Aucune compétence" />
        ) : (
          <Card className="divide-y divide-line">
            {data.skills.map((s) => (
              <div key={s.id} className="px-4 py-3">
                <div className="flex flex-wrap items-center gap-2">
                  <p className="font-mono text-sm">{s.name}</p>
                  <Badge tone={s.status === 'active' ? 'ok' : 'neutral'}>
                    {s.status === 'active' ? 'Active' : s.status}
                  </Badge>
                  {s.activeVersion && <Badge>v{s.activeVersion}</Badge>}
                </div>
                <p className="mt-1 text-sm text-muted">{s.description}</p>
              </div>
            ))}
          </Card>
        )
      }
    </Loaded>
  );
}
