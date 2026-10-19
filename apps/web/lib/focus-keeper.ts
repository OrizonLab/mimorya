/**
 * Gives the focus back to a control that lost it by being disabled: by its
 * own pending action, by the lock of a reloading screen (ADR-042, ADR-043),
 * or otherwise (the chat's composer during a run). Chromium moves such a
 * focus to the page itself, where Enter or Space do nothing and a screen
 * reader loses its place. Once the control is enabled again, and unless
 * another element got the focus meanwhile, it gets the focus back.
 */
export function keepFocusAcrossDisabling(): () => void {
  let lost: HTMLElement | null = null;

  // Losing the focus to nothing while disabled: the browser took it. Any
  // other loss (a control the user left) cancels what was owed.
  const onFocusOut = (event: FocusEvent) => {
    const target = event.target;
    const disabled = target instanceof HTMLElement && target.matches(':disabled');
    lost = disabled && event.relatedTarget === null ? (target as HTMLElement) : null;
  };
  // The focus went somewhere, even from the page itself (which emits no
  // focusout): nothing is owed any more.
  const onFocusIn = () => {
    lost = null;
  };
  // A control is enabled again (its own attribute, or the fieldset's).
  const observer = new MutationObserver(() => {
    if (!lost) return;
    if (!lost.isConnected) {
      lost = null;
      return;
    }
    if (lost.matches(':disabled')) return;
    const control = lost;
    lost = null;
    const active = document.activeElement;
    if (active === null || active === document.body) control.focus({ preventScroll: true });
  });

  document.addEventListener('focusout', onFocusOut);
  document.addEventListener('focusin', onFocusIn);
  observer.observe(document.body, {
    subtree: true,
    attributes: true,
    attributeFilter: ['disabled'],
  });
  return () => {
    document.removeEventListener('focusout', onFocusOut);
    document.removeEventListener('focusin', onFocusIn);
    observer.disconnect();
  };
}
