'use client';

import { useState, type ComponentType } from 'react';
import {
  ApprovalRoutingTab,
  AuditTab,
  ConnectorsTab,
  IdentityProvidersTab,
  LlmTab,
  OrganizationTab,
  PoliciesTab,
  RolesTab,
  SkillsTab,
  TabBar,
  UsageTab,
  UsersTab,
} from '@/components/admin';
import { EmptyState, PageHeader } from '@/components/ui';
import { visibleAdminSections, type AdminSectionKey } from '@/lib/admin-sections';
import { useMe } from '@/lib/me-context';

const SECTION_VIEWS: Record<AdminSectionKey, ComponentType> = {
  org: OrganizationTab,
  users: UsersTab,
  roles: RolesTab,
  policies: PoliciesTab,
  approvalRouting: ApprovalRoutingTab,
  audit: AuditTab,
  usage: UsageTab,
  llm: LlmTab,
  identityProviders: IdentityProvidersTab,
  connectors: ConnectorsTab,
  skills: SkillsTab,
};

export default function AdminPage() {
  const me = useMe();
  const sections = visibleAdminSections(me.permissions);
  const [active, setActive] = useState<AdminSectionKey | null>(null);
  const current = sections.find((s) => s.key === active) ?? sections[0];
  const View = current ? SECTION_VIEWS[current.key] : null;

  return (
    <div className="mx-auto w-full max-w-6xl px-4 py-8">
      <PageHeader
        title="Administration"
        description="Organisation, habilitations, traçabilité et consommation de votre espace. Les écrans affichés dépendent de vos permissions."
      />
      <TabBar tabs={sections} active={current?.key} onSelect={setActive} />
      {View ? <View /> : <EmptyState title="Aucun écran d’administration accessible" />}
    </div>
  );
}
