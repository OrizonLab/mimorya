'use client';

import { brandImageUrl } from '@/lib/brand';
import { useBrand } from './brand-context';

/** The product's logo, if the installation gives one, beside its name. */
export function BrandMark() {
  const brand = useBrand();
  return (
    <span className="inline-flex min-w-0 items-center gap-2">
      {brand.logo && (
        <img
          src={brandImageUrl('logo', brand.logo)}
          alt=""
          className="h-7 w-auto max-w-24 shrink-0 object-contain"
        />
      )}
      <span className="truncate">{brand.productName}</span>
    </span>
  );
}
