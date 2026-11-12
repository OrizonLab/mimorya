import type { SignIn } from '@/lib/api';

/** The page of a sign-in that waits for its second factor, to prove or to enrol (ADR-200, ADR-201). */
export const SECOND_FACTOR_PAGE = '/login/second-factor';

/**
 * Where a password checked leads (a login, the end of a reset, an accepted
 * invitation): the application once the session is open, else the page
 * that asks for the second factor.
 */
export function pathAfterSignIn(answer: SignIn): string {
  return answer.next === 'signed_in' ? '/chat' : SECOND_FACTOR_PAGE;
}
