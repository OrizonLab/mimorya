'use client';

import Link from 'next/link';

/**
 * Which connectors the run goes without, and what it means for the answer
 * (ADR-048). Those each person authorizes for themself, which the user has
 * not, are said apart, with the way to their account, where they authorize
 * them (ADR-174).
 */
export function UnavailableNotices({
  unavailable,
  toAuthorize,
}: {
  unavailable: string[];
  toAuthorize: string[];
}) {
  const theirs = new Set(toAuthorize);
  const others = unavailable.filter((name) => !theirs.has(name));
  return (
    <>
      {others.length > 0 && <p className="mt-2 text-xs text-warn">{unavailableNotice(others)}</p>}
      {toAuthorize.length > 0 && (
        <p className="mt-2 text-xs text-warn">
          {toAuthorizeNotice(toAuthorize)}{' '}
          <Link href="/account" className="font-medium text-accent underline">
            {toAuthorize.length === 1 ? 'L’autoriser' : 'Les autoriser'} dans « Mon compte »
          </Link>
        </p>
      )}
    </>
  );
}

function named(connectors: string[]): string {
  return connectors.map((name) => `« ${name} »`).join(', ');
}

function unavailableNotice(connectors: string[]): string {
  return connectors.length === 1
    ? `Le connecteur ${named(connectors)} est indisponible : la réponse peut être incomplète.`
    : `Les connecteurs ${named(connectors)} sont indisponibles : la réponse peut être incomplète.`;
}

function toAuthorizeNotice(connectors: string[]): string {
  return connectors.length === 1
    ? `Le connecteur ${named(connectors)} s’utilise avec votre propre autorisation, que vous n’avez pas donnée : la réponse peut être incomplète.`
    : `Les connecteurs ${named(connectors)} s’utilisent avec votre propre autorisation, que vous n’avez pas donnée : la réponse peut être incomplète.`;
}
