import { cx } from '@/components/ui';

export function TabBar<K extends string>({
  tabs,
  active,
  onSelect,
}: {
  tabs: ReadonlyArray<{ key: K; label: string }>;
  active: K | undefined;
  onSelect: (key: K) => void;
}) {
  return (
    <div className="mb-6 flex gap-1 overflow-x-auto border-b border-line" role="tablist">
      {tabs.map((t) => (
        <button
          key={t.key}
          type="button"
          role="tab"
          aria-selected={active === t.key}
          onClick={() => onSelect(t.key)}
          className={cx(
            '-mb-px border-b-2 px-3 py-2 text-sm whitespace-nowrap',
            active === t.key
              ? 'border-accent font-medium text-accent'
              : 'border-transparent text-muted hover:text-fg',
          )}
        >
          {t.label}
        </button>
      ))}
    </div>
  );
}
