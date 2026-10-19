'use client';

import { Mail } from 'lucide-react';
import { Button, Card } from '@/components/ui';
import { formatDate } from '@/lib/format';

/** A reset link just e-mailed to an account: the administrator never sees it. */
export function ResetLinkNotice({
  who,
  email,
  expiresAt,
  onClose,
}: {
  who: string;
  email: string;
  expiresAt: string;
  onClose: () => void;
}) {
  return (
    <Card className="mb-4 flex flex-col gap-2 p-4">
      <p className="flex items-center gap-2 text-sm font-medium">
        <Mail className="size-4" aria-hidden /> Lien de réinitialisation de {who}
      </p>
      <p className="text-sm">
        Lien envoyé à {email}, valable jusqu’au {formatDate(expiresAt)}. Son mot de passe actuel
        reste valable jusqu’à l’utilisation du lien.
      </p>
      <Button variant="secondary" className="w-fit" onClick={onClose}>
        Fermer
      </Button>
    </Card>
  );
}
