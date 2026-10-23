import {
  Activity,
  BookOpen,
  CalendarClock,
  CheckCircle2,
  MessageSquare,
  Settings,
} from 'lucide-react';
import { visibleAdminSections } from '@/lib/admin-sections';
import type { Me } from '@/lib/api';

export interface NavItem {
  href: string;
  label: string;
  icon: typeof MessageSquare;
  visible?: (me: Me) => boolean;
}

export const NAV_ITEMS: NavItem[] = [
  { href: '/chat', label: 'Conversations', icon: MessageSquare },
  { href: '/approvals', label: 'Validations', icon: CheckCircle2 },
  { href: '/scheduled-tasks', label: 'Tâches planifiées', icon: CalendarClock },
  { href: '/operations', label: 'Opérations', icon: Activity },
  { href: '/memory', label: 'Mémoire', icon: BookOpen },
  {
    href: '/admin',
    label: 'Administration',
    icon: Settings,
    visible: (me) => visibleAdminSections(me.permissions).length > 0,
  },
];
