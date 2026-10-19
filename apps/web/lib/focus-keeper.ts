/**
 * Gives the focus back to a control that lost it by being disabled: by its
 * own pending action, or by the lock of a reloading screen (ADR-042, ADR-043).
 * The browser moves such a focus to the page itself, from where a keyboard
 * user would start again from the top. Once the control is enabled again,
 * and unless the focus went elsewhere meanwhile, it gets the focus back.
 */
export function keepFocusAcrossDisabling(): () => void {
  let lost: HTMLElement | null = null;

  // Losing the focus to nothing while disabled: the browser took it.
  const onFocusOut = (event: FocusEvent) => {
    const target = event.target;
    const disabled = target instanceof HTMLElement && target.matches(':disabled');
    lost = disabled && event.relatedTarget === null ? (target as HTMLElement) : null;
  };
  // The focus went somewhere: nothing is owed any more.
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
