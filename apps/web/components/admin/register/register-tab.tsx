'use client';

import { Button, Card, Loaded } from '@/components/ui';
import { useApiData, type ProcessingRegisterView } from '@/lib/api';
import { formatDate } from '@/lib/format';
import { saveFile } from '@/lib/save-file';
import { ActivityCard } from './activity-card';
import { registerCsv, registerFileName } from './register-csv';

/**
 * The company's record of processing activities (RGPD art. 30, ADR-217,
 * ADR-218), drawn from its settings each time the tab opens: read here,
 * downloaded as a spreadsheet, completed by the company with what it alone
 * knows.
 */
export function RegisterTab() {
  const state = useApiData<ProcessingRegisterView>('/admin/processing-register');
  return <Loaded state={state}>{(data) => <Register data={data} />}</Loaded>;
}

function Register({ data }: { data: ProcessingRegisterView }) {
  function download() {
    const blob = new Blob([registerCsv(data)], { type: 'text/csv;charset=utf-8' });
    saveFile({ name: registerFileName(data), blob });
  }

  return (
    <div className="flex flex-col gap-4">
      <Card className="p-4">
        <section aria-label="Registre des traitements" className="flex flex-col gap-3">
          <p className="font-medium">Registre des traitements</p>
          <p className="text-sm">
            Responsable du traitement : <span className="font-semibold">{data.controller}</span>
          </p>
          <p className="text-sm text-muted">
            Ce que la plateforme traite pour votre entreprise, tiré de vos réglages le{' '}
            {formatDate(data.drawnAt)} : il suit chaque changement de vos modèles, connecteurs,
            passerelles et durées de conservation. La localisation d’un fournisseur est ce que la
            plateforme en sait ; votre contrat avec lui dit où il traite vos données.
          </p>
          <Button type="button" className="w-fit" onClick={download}>
            Télécharger (CSV)
          </Button>
        </section>
      </Card>
      {data.activities.map((activity) => (
        <ActivityCard key={activity.key} activity={activity} />
      ))}
      <Card className="p-4">
        <section aria-labelledby="register-measures" className="flex flex-col gap-2">
          <h3 id="register-measures" className="font-medium">
            Mesures de sécurité
          </h3>
          <ul className="list-disc pl-5 text-sm">
            {data.measures.map((measure) => (
              <li key={measure}>{measure}</li>
            ))}
          </ul>
        </section>
      </Card>
      <Card className="p-4">
        <section aria-labelledby="register-to-complete" className="flex flex-col gap-2">
          <h3 id="register-to-complete" className="font-medium">
            À compléter par votre entreprise
          </h3>
          <ul className="list-disc pl-5 text-sm">
            {data.toComplete.map((item) => (
              <li key={item}>{item}</li>
            ))}
          </ul>
        </section>
      </Card>
    </div>
  );
}
