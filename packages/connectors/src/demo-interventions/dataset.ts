/**
 * FICTIONAL intervention reports for the demo company "Néo Maintenance".
 * Dates are generated relative to a reference date so that "this week"
 * always has data. Some records deliberately contain inconsistencies
 * (status vs. report, missing signature, empty report) to exercise analysis.
 */
import type { Sensitivity } from '@mimorya/core';
import { DEMO_UNITS, DEMO_USERS } from '@mimorya/core/demo';
import { isoDate, mondayOf } from './dates';

export type InterventionStatus = 'planned' | 'in_progress' | 'completed' | 'cancelled';

export interface Intervention {
  id: string;
  technicianId: string;
  teamUnitId: string;
  date: string; // YYYY-MM-DD
  startTime: string; // HH:MM
  durationHours: number;
  type: string;
  client: string;
  city: string;
  status: InterventionStatus;
  report: string;
  partsPending: string | null;
  clientSignature: boolean;
  sensitivity: Sensitivity;
}

export interface Absence {
  technicianId: string;
  date: string;
  startTime: string;
  endTime: string;
  reason: string;
}
function dayOffset(monday: Date, offset: number): string {
  const d = new Date(monday);
  d.setUTCDate(d.getUTCDate() + offset);
  return isoDate(d);
}

const N = DEMO_UNITS.teamNorth;
const S = DEMO_UNITS.teamSouth;
const U = DEMO_USERS;

export function buildDemoDataset(reference: Date): {
  interventions: Intervention[];
  absences: Absence[];
} {
  const monday = mondayOf(reference);
  const d = (offset: number) => dayOffset(monday, offset);
  const base = {
    partsPending: null,
    clientSignature: true,
    sensitivity: 'internal' as Sensitivity,
  };

  const interventions: Intervention[] = [
    // Previous week
    {
      ...base,
      id: 'INT-2031',
      technicianId: U.lucas,
      teamUnitId: N,
      date: d(-3),
      startTime: '09:00',
      durationHours: 2,
      type: 'Dépannage chaudière',
      client: 'Copropriété Les Acacias',
      city: 'Roubaix',
      status: 'completed',
      report: 'Thermocouple remplacé, chaudière remise en service. Client satisfait.',
    },
    {
      ...base,
      id: 'INT-2033',
      technicianId: U.emma,
      teamUnitId: N,
      date: d(-4),
      startTime: '14:00',
      durationHours: 3,
      type: 'Dépannage pompe à chaleur',
      client: 'Gîte du Moulin',
      city: 'Bondues',
      status: 'in_progress',
      report: 'Carte électronique défectueuse. Attente de la pièce chez le fournisseur.',
      partsPending: 'Carte électronique PAC',
      clientSignature: false,
    },
    // Current week — Équipe Nord
    {
      ...base,
      id: 'INT-2041',
      technicianId: U.lucas,
      teamUnitId: N,
      date: d(0),
      startTime: '08:30',
      durationHours: 2,
      type: 'Entretien chaudière gaz',
      client: 'Boulangerie Le Fournil',
      city: 'Lille',
      status: 'completed',
      report: 'Entretien annuel réalisé, brûleur nettoyé, contrôle CO conforme.',
    },
    {
      ...base,
      id: 'INT-2042',
      technicianId: U.emma,
      teamUnitId: N,
      date: d(0),
      startTime: '10:00',
      durationHours: 3,
      type: 'Dépannage climatisation',
      client: 'Cabinet dentaire Dr Morel',
      city: 'Lille',
      status: 'in_progress',
      report:
        'Fuite de fluide détectée sur l’unité extérieure. Recharge impossible sans remplacer la vanne Schrader. Commande passée.',
      partsPending: 'Vanne Schrader',
      clientSignature: false,
    },
    {
      ...base,
      id: 'INT-2043',
      technicianId: U.lucas,
      teamUnitId: N,
      date: d(1),
      startTime: '09:00',
      durationHours: 3,
      type: 'Remplacement circulateur',
      client: 'Résidence Les Tilleuls',
      city: 'Villeneuve-d’Ascq',
      status: 'completed',
      report:
        'Circulateur hors service confirmé. Pièce à commander : circulateur Grundfos. Remise en service provisoire sur le circulateur de secours.',
      partsPending: 'Circulateur Grundfos',
    },
    {
      ...base,
      id: 'INT-2044',
      technicianId: U.julien,
      teamUnitId: N,
      date: d(1),
      startTime: '14:00',
      durationHours: 2,
      type: 'Audit installation',
      client: 'Mairie de Marcq',
      city: 'Marcq-en-Barœul',
      status: 'completed',
      report:
        'Audit des installations de chauffage de la salle des fêtes. Rapport transmis au service technique de la mairie.',
    },
    {
      ...base,
      id: 'INT-2045',
      technicianId: U.emma,
      teamUnitId: N,
      date: d(2),
      startTime: '08:00',
      durationHours: 2,
      type: 'Entretien VMC',
      client: 'Crèche Les Petits Pas',
      city: 'Lille',
      status: 'completed',
      report: 'Filtres remplacés, débits contrôlés.',
      clientSignature: false,
    },
    {
      ...base,
      id: 'INT-2046',
      technicianId: U.lucas,
      teamUnitId: N,
      date: d(3),
      startTime: '13:30',
      durationHours: 2,
      type: 'Dépannage chauffe-eau',
      client: 'M. et Mme Lambert',
      city: 'Tourcoing',
      status: 'in_progress',
      report: '',
      clientSignature: false,
    },
    {
      ...base,
      id: 'INT-2047',
      technicianId: U.emma,
      teamUnitId: N,
      date: d(4),
      startTime: '09:00',
      durationHours: 4,
      type: 'Mise en service pompe à chaleur',
      client: 'Hôtel du Parc',
      city: 'Lille',
      status: 'planned',
      report: '',
      clientSignature: false,
    },
    // Current week — Équipe Sud
    {
      ...base,
      id: 'INT-2048',
      technicianId: U.hugo,
      teamUnitId: S,
      date: d(0),
      startTime: '08:00',
      durationHours: 2,
      type: 'Entretien chaudière fioul',
      client: 'Ferme Delattre',
      city: 'Seclin',
      status: 'completed',
      report: 'Entretien réalisé, gicleur remplacé.',
    },
    {
      ...base,
      id: 'INT-2049',
      technicianId: U.lea,
      teamUnitId: S,
      date: d(1),
      startTime: '10:00',
      durationHours: 2,
      type: 'Dépannage plancher chauffant',
      client: 'École Jules Ferry',
      city: 'Wattignies',
      status: 'in_progress',
      report:
        'Accès à la chaufferie impossible : clé non disponible. Nouveau passage à prévoir avec le gardien.',
      clientSignature: false,
    },
    {
      ...base,
      id: 'INT-2050',
      technicianId: U.hugo,
      teamUnitId: S,
      date: d(2),
      startTime: '14:00',
      durationHours: 1,
      type: 'Remplacement thermostat',
      client: 'Pharmacie Centrale',
      city: 'Lesquin',
      status: 'completed',
      report: 'Thermostat connecté installé et paramétré avec le client.',
    },
    {
      ...base,
      id: 'INT-2051',
      technicianId: U.nadia,
      teamUnitId: S,
      date: d(2),
      startTime: '09:00',
      durationHours: 3,
      type: 'Diagnostic pompe de relevage',
      client: 'Site industriel Valmet',
      city: 'Templemars',
      status: 'in_progress',
      report: 'Pompe de relevage défaillante. Devis de remplacement à faire valider par le client.',
      partsPending: 'Pompe de relevage',
      clientSignature: false,
      sensitivity: 'confidential',
    },
    {
      ...base,
      id: 'INT-2052',
      technicianId: U.lea,
      teamUnitId: S,
      date: d(3),
      startTime: '11:00',
      durationHours: 2,
      type: 'Entretien chaudière',
      client: 'Restaurant La Table',
      city: 'Faches-Thumesnil',
      status: 'completed',
      report: 'Client absent au rendez-vous, intervention non réalisée. À reprogrammer.',
      clientSignature: false,
    },
    {
      ...base,
      id: 'INT-2053',
      technicianId: U.hugo,
      teamUnitId: S,
      date: d(4),
      startTime: '08:30',
      durationHours: 3,
      type: 'Entretien chaufferie collective',
      client: 'Résidence Le Belvédère',
      city: 'Seclin',
      status: 'planned',
      report: '',
      clientSignature: false,
    },
    // Next week (already planned)
    {
      ...base,
      id: 'INT-2061',
      technicianId: U.lucas,
      teamUnitId: N,
      date: d(7),
      startTime: '08:00',
      durationHours: 4,
      type: 'Entretien chaufferie',
      client: 'Lycée Pasteur',
      city: 'Lille',
      status: 'planned',
      report: '',
      clientSignature: false,
    },
    {
      ...base,
      id: 'INT-2062',
      technicianId: U.emma,
      teamUnitId: N,
      date: d(8),
      startTime: '13:00',
      durationHours: 3,
      type: 'Dépannage climatisation',
      client: 'Agence Immo Nord',
      city: 'Lille',
      status: 'planned',
      report: '',
      clientSignature: false,
    },
    {
      ...base,
      id: 'INT-2063',
      technicianId: U.hugo,
      teamUnitId: S,
      date: d(7),
      startTime: '09:00',
      durationHours: 2,
      type: 'Entretien chaudière gaz',
      client: 'Cabinet Avocats Martel',
      city: 'Lesquin',
      status: 'planned',
      report: '',
      clientSignature: false,
    },
  ];

  const absences: Absence[] = [
    {
      technicianId: U.emma,
      date: d(10),
      startTime: '08:00',
      endTime: '17:00',
      reason: 'Formation',
    },
    { technicianId: U.hugo, date: d(11), startTime: '13:00', endTime: '17:00', reason: 'Congé' },
    {
      technicianId: U.lucas,
      date: d(9),
      startTime: '08:00',
      endTime: '12:00',
      reason: 'Visite médicale',
    },
  ];

  return { interventions, absences };
}
