import Link from 'next/link';
import { AuthFrame } from '@/components/account';
import { Card } from '@/components/ui';

/**
 * Any unknown address. Next.js's own page styles itself inline, which the
 * Content Security Policy refuses (ADR-030): this one only uses classes.
 */
export default function NotFound() {
  return (
    <AuthFrame>
      <Card className="flex flex-col gap-3 p-6 text-sm">
        <p className="font-medium">Page introuvable</p>
        <p className="text-muted">Cette adresse ne correspond à aucune page.</p>
        <Link href="/" className="w-fit text-accent hover:underline">
          Retour à l’accueil
        </Link>
      </Card>
    </AuthFrame>
  );
}
