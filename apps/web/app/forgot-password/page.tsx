import Link from 'next/link';
import { AuthFrame, ResetRequestForm } from '@/components/account';

/** Asks for a password reset link, before any sign-in. */
export default function ForgotPasswordPage() {
  return (
    <AuthFrame>
      <ResetRequestForm />
      <p className="mt-4 text-center text-sm">
        <Link href="/login" className="text-muted hover:text-fg">
          Retour à la connexion
        </Link>
      </p>
    </AuthFrame>
  );
}
