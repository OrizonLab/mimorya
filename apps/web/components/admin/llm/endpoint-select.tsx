'use client';

import { Field, Select } from '@/components/ui';

/** The server of a self-hosted provider, among those the operator lists. */
export function EndpointSelect({
  label,
  endpoints,
  value,
  onChange,
}: {
  label: string;
  endpoints: string[];
  value: string;
  onChange: (url: string) => void;
}) {
  return (
    <Field label={label}>
      <Select value={value} onChange={(e) => onChange(e.target.value)}>
        {endpoints.map((url) => (
          <option key={url} value={url}>
            {url}
          </option>
        ))}
      </Select>
    </Field>
  );
}
