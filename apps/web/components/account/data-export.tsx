'use client';

import { useState } from 'react';
import { Button, Card, ErrorNote } from '@/components/ui';
import { api, useAction } from '@/lib/api';
import { saveFile } from '@/lib/save-file';

const EXPORT = '/account/export';

/**
 * The signed-in person's own data, downloaded as a JSON file (ADR-187,
 * ADR-188): what the platform keeps on them and what they wrote or had
 * done, never a secret nor another's data. A server makes a few exports at
 * once, one per person: its refusal says so.
 */
export function DataExport() {
  const action = useAction();
  const [done, setDone] = useState(false);

  async function download() {
    setDone(false);
    const file = await action.run(() => api.download(EXPORT, 'mimorya-mes-donnees.json'));
    if (file) {
      saveFile(file);
      setDone(true);
    }
  }

  return (
    <Card className="max-w-2xl p-4">
      <section aria-labelledby="data-export-title" className="flex flex-col gap-3">
        <p id="data-export-title" className="font-medium">
          Mes données
        </p>
        <p className="text-sm text-muted">
          Téléchargez, en un fichier JSON, ce que la plateforme garde sur vous et ce que vous y avez
          fait : votre compte et vos connexions, vos conversations, vos mémoires, vos tâches,
          déclencheurs et missions, ce que l’assistant a fait pour vous et les validations qui vous
          concernent. Jamais un mot de passe ni un jeton, jamais les données d’un collègue. Chaque
          téléchargement est inscrit au journal d’audit de votre entreprise.
        </p>
        <p className="text-xs text-muted">
          L’effacement de votre compte n’est pas encore proposé : votre administrateur peut le
          désactiver.
        </p>
        {action.error && <ErrorNote message={action.error} />}
        {done && (
          <p role="status" className="text-sm text-ok">
            Vos données sont téléchargées.
          </p>
        )}
        <Button type="button" className="w-fit" loading={action.pending} onClick={download}>
          Télécharger mes données
        </Button>
      </section>
    </Card>
  );
}
