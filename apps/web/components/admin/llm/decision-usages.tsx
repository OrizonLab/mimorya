'use client';

import { Field, Input, Select } from '@/components/ui';
import type { DecisionModeChoice, DecisionUsageUpdate, DecisionUsageView } from '@/lib/api';

const MODES: { mode: DecisionModeChoice; label: string }[] = [
  { mode: 'off', label: 'Désactivé' },
  { mode: 'observe', label: 'Observation (sans effet)' },
  { mode: 'active', label: 'Actif' },
];

/** The usages as the administrator sets them, from those read. */
export function usagesOf(usages: DecisionUsageView[]): DecisionUsageUpdate[] {
  return usages.map((usage) => ({
    key: usage.key,
    mode: usage.mode,
    threshold: usage.threshold,
  }));
}

/** Each usage of the decision service (ADR-213), its mode and threshold:
 * one whose step is not delivered stays off. */
export function DecisionUsagesFields({
  usages,
  values,
  onChange,
}: {
  usages: DecisionUsageView[];
  values: DecisionUsageUpdate[];
  onChange: (values: DecisionUsageUpdate[]) => void;
}) {
  function update(index: number, changed: Partial<DecisionUsageUpdate>) {
    onChange(values.map((value, at) => (at === index ? { ...value, ...changed } : value)));
  }
  return (
    <fieldset className="flex flex-col gap-3">
      <legend className="mb-2 text-sm font-medium">Usages</legend>
      <p className="text-xs text-muted">
        Le choix du modèle d’un run ne sert que là où le routeur de l’escalade classe la demande :
        actif, « escalade », ou « principal » avec une confiance sous le seuil, fait partir le run
        sur le modèle d’escalade, dans son budget ; sans décision, le classement par le modèle
        principal s’applique comme avant. Le choix des compétences, actif, ne liste avec la demande
        que celles qui lui sont utiles, toutes en cas de doute ; chacune reste chargeable. Les
        données sensibles, actif : une mémoire personnelle qui en contient, ou que la décision ne
        peut lire, attend la validation de son propriétaire, et une leçon qui en contient n’est pas
        gardée ; baisser son seuil rend cette garde plus stricte. Le classement des documents, actif
        : après chaque dépôt, le service lit le titre et les trois premières pages ; une
        confidentialité plus stricte est appliquée aussitôt, une plus basse et le type sont
        seulement proposés à qui peut écrire le document ; baisser son seuil retient plus souvent un
        niveau strict. Les actions sensibles, actif : avant qu’une action qui agit parte ou soit
        mise en validation, le service lit l’outil et ses paramètres ; ce qu’il signale
        (destinataire externe, montant élevé, données personnelles, action irréversible, risque
        élevé) s’ajoute à la raison de la validation, et une action qui partirait seule attend une
        validation ; baisser son seuil le rend plus strict.
      </p>
      {usages.map((usage, index) => {
        const value = values[index];
        if (!value) return null;
        return (
          <div key={usage.key} className="grid gap-2 sm:grid-cols-[1fr_14rem_8rem] sm:items-end">
            <p className="text-sm">
              {usage.label}
              {!usage.available && (
                <span className="text-xs text-muted"> (bientôt disponible)</span>
              )}
            </p>
            <Field label="Mode">
              <Select
                aria-label={`Mode : ${usage.label}`}
                value={value.mode}
                disabled={!usage.available}
                onChange={(e) => update(index, { mode: e.target.value as DecisionModeChoice })}
              >
                {MODES.map(({ mode, label }) => (
                  <option key={mode} value={mode}>
                    {label}
                  </option>
                ))}
              </Select>
            </Field>
            <Field label="Seuil">
              <Input
                aria-label={`Seuil : ${usage.label}`}
                type="number"
                required
                min={0}
                max={1}
                step={0.05}
                disabled={!usage.available}
                value={String(value.threshold)}
                onChange={(e) => update(index, { threshold: Number(e.target.value) })}
              />
            </Field>
          </div>
        );
      })}
    </fieldset>
  );
}
