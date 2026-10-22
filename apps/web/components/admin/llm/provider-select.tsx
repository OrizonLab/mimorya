'use client';

import { Field, Select } from '@/components/ui';

/** A kind of provider a company may choose, as far as the form goes. */
type Kind = { kind: string; requiresKey: boolean; listedEndpoint: boolean };

/** The provider, among those the server can serve: a listed server needs
 * the operator's list, a key the server's master key. */
export function ProviderSelect({
  label,
  names,
  providers,
  endpoints,
  secretsAvailable,
  value,
  onChange,
}: {
  label: string;
  names: Record<string, string>;
  providers: Kind[];
  endpoints: string[];
  secretsAvailable: boolean;
  value: string;
  onChange: (kind: string) => void;
}) {
  return (
    <Field label={label}>
      <Select value={value} onChange={(e) => onChange(e.target.value)}>
        {providers.map((p) => (
          <option
            key={p.kind}
            value={p.kind}
            disabled={
              (p.listedEndpoint && endpoints.length === 0) || (p.requiresKey && !secretsAvailable)
            }
          >
            {names[p.kind] ?? p.kind}
          </option>
        ))}
      </Select>
    </Field>
  );
}
