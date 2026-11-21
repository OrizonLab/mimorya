import type { ProcessingRegisterView, RegisterActivity } from '@/lib/api';
import { PLATFORM_TIME_ZONE } from '@/lib/format';

/** The columns of the downloaded register, one row per recipient. */
const COLUMNS = [
  'Responsable du traitement',
  'Traitement',
  'Finalité',
  'Personnes concernées',
  'Catégories de données',
  'Données sensibles',
  'Destinataire',
  'Reçoit les données pour',
  'Localisation',
  'Choisi par',
  'Durées de conservation',
  'Remarques',
];

/** Tells a spreadsheet the file is in UTF-8. */
const BYTE_ORDER_MARK = '\uFEFF';

/** The parts of a day, as the platform counts days. */
const DAY = new Intl.DateTimeFormat('fr-FR', {
  timeZone: PLATFORM_TIME_ZONE,
  year: 'numeric',
  month: '2-digit',
  day: '2-digit',
});

/** `instant`'s day in Paris, as `AAAA-MM-JJ`, whatever a locale's pattern. */
function parisDay(instant: Date): string {
  const parts = Object.fromEntries(
    DAY.formatToParts(instant).map((part) => [part.type, part.value]),
  );
  return `${parts.year}-${parts.month}-${parts.day}`;
}

/** A spreadsheet reads a cell opening with one of these as a formula. */
const FORMULA_START = /^[=+\-@\t\r]/;

/**
 * The register's activities as a CSV file a spreadsheet opens (ADR-218):
 * separated by semicolons, in UTF-8 with its byte order mark, one row per
 * recipient of each activity (one, its recipient left empty, for an
 * activity without any), each naming the controller. A cell is quoted, and
 * one a spreadsheet would read as a formula (the company's name, should it
 * begin so) is kept as text: a defence in depth, the platform's own texts
 * never beginning so.
 */
export function registerCsv(register: ProcessingRegisterView): string {
  const rows = register.activities.flatMap((activity) =>
    activityRows(activity).map((row) => [register.controller, ...row]),
  );
  const lines = [COLUMNS, ...rows].map((row) => row.map(cell).join(';'));
  return `${BYTE_ORDER_MARK}${lines.join('\r\n')}\r\n`;
}

/** The file's name, dated the day the register was drawn, in Paris. */
export function registerFileName(register: ProcessingRegisterView): string {
  return `registre-des-traitements-${parisDay(new Date(register.drawnAt))}.csv`;
}

function activityRows(activity: RegisterActivity): string[][] {
  const head = [
    activity.name,
    activity.purpose,
    activity.subjects.join('\n'),
    activity.data.join('\n'),
    activity.sensitive ?? '',
  ];
  const tail = [activity.retention.join('\n'), activity.notes.join('\n')];
  if (activity.recipients.length === 0) {
    return [[...head, 'Aucun hors de la plateforme', '', '', '', ...tail]];
  }
  return activity.recipients.map((recipient) => [
    ...head,
    recipient.name,
    recipient.role,
    recipient.placeLabel,
    recipient.chosenByLabel,
    ...tail,
  ]);
}

function cell(value: string): string {
  const text = FORMULA_START.test(value) ? `'${value}` : value;
  return `"${text.replaceAll('"', '""')}"`;
}
