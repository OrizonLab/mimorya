import { Field, Select } from '@/components/ui';
import type { RouterRoute, RouterRoutes } from '@/lib/api';
import { ROUTER_ROUTE_LABELS, RUN_KIND_LABELS } from '@/lib/labels';

/** Every kind of run on the main model: where a company's router starts. */
export const MAIN_ROUTES: RouterRoutes = {
  conversation: 'main',
  mission: 'main',
  scheduledTask: 'main',
  hook: 'main',
};

const KINDS = Object.keys(RUN_KIND_LABELS) as (keyof RouterRoutes)[];
const ROUTES = Object.keys(ROUTER_ROUTE_LABELS) as RouterRoute[];

/** The model each kind of run starts with, while the router is on (ADR-207). */
export function RouterRoutesFields({
  routes,
  onChange,
}: {
  routes: RouterRoutes;
  onChange: (routes: RouterRoutes) => void;
}) {
  return (
    <fieldset className="flex flex-col gap-2 pl-6">
      <legend className="text-xs text-muted">
        Modèle de départ de chaque sorte de run. Une compétence qui demande le modèle d’escalade y
        fait passer le run quand l’agent la charge. Le classement est un court appel au modèle
        principal, qui ne lit que la demande ; son coût entre dans le budget d’escalade, et toute
        réponse illisible garde le modèle principal.
      </legend>
      {KINDS.map((kind) => (
        <Field key={kind} label={RUN_KIND_LABELS[kind]}>
          <Select
            value={routes[kind]}
            onChange={(e) => onChange({ ...routes, [kind]: e.target.value as RouterRoute })}
          >
            {ROUTES.map((route) => (
              <option key={route} value={route}>
                {ROUTER_ROUTE_LABELS[route]}
              </option>
            ))}
          </Select>
        </Field>
      ))}
    </fieldset>
  );
}
