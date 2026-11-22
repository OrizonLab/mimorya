import type { BrandImageView, BrandPalette, BrandView } from '@/lib/api';

/** The brand when the installation sets none, or before the API answers (ADR-222). */
export const DEFAULT_BRAND: BrandView = {
  productName: 'Mimorya',
  assistantName: 'Mimorya',
  filePrefix: 'mimorya',
  logo: null,
  favicon: null,
  light: null,
  dark: null,
};

/** The address of an image of the brand: it changes with the image. */
export function brandImageUrl(kind: 'logo' | 'favicon', image: BrandImageView): string {
  return `/api/brand/${kind}?v=${encodeURIComponent(image.version)}`;
}

const COLOUR = /^#[0-9a-f]{6}$/;

/**
 * The brand's accents over the interface's own tokens (`globals.css`), for
 * the scheme each is set for; nothing when none is. `:root:root` wins over
 * the interface's `:root` whatever order the sheets load in. The colours
 * were checked by the API; only `#rrggbb` is written here all the same.
 */
export function brandStyle(brand: BrandView): string {
  const tokens = (palette: BrandPalette | null) => {
    if (!palette) return '';
    const colours = [palette.accent, palette.accentFg, palette.accentSoft];
    if (!colours.every((colour) => COLOUR.test(colour))) return '';
    return `:root:root{--accent:${palette.accent};--accent-fg:${palette.accentFg};--accent-soft:${palette.accentSoft}}`;
  };
  const light = tokens(brand.light);
  const dark = tokens(brand.dark);
  return (
    (light && `@media (prefers-color-scheme: light){${light}}`) +
    (dark && `@media (prefers-color-scheme: dark){${dark}}`)
  );
}
