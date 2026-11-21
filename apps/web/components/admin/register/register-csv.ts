import type { ProcessingRegisterView, RegisterActivity } from '@/lib/api';

/** The columns of the downloaded register, one row per recipient. */
const COLUMNS = [
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

/** A spreadsheet reads a cell opening with one of these as a formula. */
const FORMULA_START = /^[=+\-@\t\r]/;

/**
 * The register's activities as a CSV file a spreadsheet opens (ADR-218):
 * separated by semicolons, in UTF-8 with its byte order mark, one row per
 * recipient of each activity (one, its recipient left empty, for an
 * activity without any). A cell is quoted, and one a spreadsheet would read
 * as a formula (a connector's name, a company's) is kept as text.
 */
export function registerCsv(register: ProcessingRegisterView): string {
  const rows = register.activities.flatMap(activityRows);
  const lines = [COLUMNS, ...rows].map((row) => row.map(cell).join(';'));
  return `${BYTE_ORDER_MARK}${lines.join('\r\n')}\r\n`;
}

/** The file's name, dated as the register was drawn. */
export function registerFileName(register: ProcessingRegisterView): string {
  return `registre-des-traitements-${register.drawnAt.slice(0, 10)}.csv`;
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
