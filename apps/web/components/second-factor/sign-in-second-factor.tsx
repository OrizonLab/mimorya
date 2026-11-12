'use client';

import Link from 'next/link';
import { useEffect, useState } from 'react';
import { Card, ErrorNote, Spinner } from '@/components/ui';
import { api, ApiError, type SignIn } from '@/lib/api';
import { SignInCode } from './sign-in-code';
import { SignInEnrolment } from './sign-in-enrolment';

const CANNOT_READ = 'Connexion impossible à reprendre : saisissez de nouveau votre mot de passe.';

/**
 * A sign-in whose password was checked and that waits for its second
 * factor (ADR-200), or for its enrolment where the company requires one
 * (ADR-201), as the browser's challenge says; the application once the
 * session is open. An ended or expired sign-in sends back to the password.
 */
export function SignInSecondFactor() {
  const [waiting, setWaiting] = useState<SignIn | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    api
      .get<SignIn>('/auth/second-factor')
      .then(setWaiting)
      .catch((e: unknown) => setError(e instanceof ApiError ? e.message : CANNOT_READ));
  }, []);

  const signedIn = () => window.location.assign('/chat');

  return (
    <Card className="p-6">
      {error ? (
        <div className="flex flex-col gap-4">
          <ErrorNote message={error} />
          <Link href="/login" className="text-center text-sm text-muted hover:text-fg">
            Revenir à la connexion
          </Link>
        </div>
      ) : waiting === null ? (
        <Spinner />
      ) : waiting.next === 'second_factor_enrolment' ? (
        <SignInEnrolment onSignedIn={signedIn} />
      ) : (
        <SignInCode onSignedIn={signedIn} />
      )}
    </Card>
  );
}
