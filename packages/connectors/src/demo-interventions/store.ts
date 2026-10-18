/** In-memory state of the demo connector: dataset plus plannings applied since start. */
import { buildDemoDataset, type Absence, type Intervention } from './dataset';

export interface ScheduledSlot {
  technicianId: string;
  date: string;
  startTime: string;
  durationHours: number;
  label: string;
}

export class DemoInterventionsStore {
  readonly interventions: Intervention[];
  readonly absences: Absence[];
  readonly applied: ScheduledSlot[] = [];

  constructor(readonly reference: Date = new Date()) {
    const data = buildDemoDataset(reference);
    this.interventions = data.interventions;
    this.absences = data.absences;
  }
}
