/**
 * Demo MCP server "documentation technique" — FICTIONAL procedures. Used by
 * tests (in-memory transport) and runnable over stdio for local demos:
 *   pnpm --filter @mimorya/connectors mcp:demo-docs
 */
import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import { z } from 'zod';

const PROCEDURES = [
  {
    id: 'PROC-12',
    title: 'Remplacement d’un circulateur de chauffage',
    content:
      'Couper l’alimentation, isoler le circuit, vidanger la portion concernée, remplacer le circulateur, purger, contrôler la pression (1,2 à 1,5 bar), remettre en service et faire signer le compte rendu.',
  },
  {
    id: 'PROC-07',
    title: 'Clôture d’une intervention',
    content:
      'Une intervention est clôturée lorsque le compte rendu est complet, signé par le client et qu’aucune pièce n’est en attente. Sinon, elle reste « en cours » et un suivi est planifié.',
  },
  {
    id: 'PROC-21',
    title: 'Recharge en fluide frigorigène',
    content:
      'Réservée aux techniciens titulaires de l’attestation de capacité. Rechercher et réparer la fuite avant toute recharge ; consigner la quantité dans le registre.',
  },
];

export function createDemoDocsServer(): McpServer {
  const server = new McpServer({ name: 'neo-docs-demo', version: '0.1.0' });
  server.registerTool(
    'search_procedures',
    {
      title: 'Recherche de procédures',
      description: 'Recherche dans la base documentaire technique (procédures internes).',
      inputSchema: { query: z.string().min(2) },
      annotations: { readOnlyHint: true },
    },
    async ({ query }) => {
      const terms = query
        .toLowerCase()
        .split(/\s+/)
        .filter((t) => t.length > 2);
      const hits = PROCEDURES.filter((p) =>
        terms.some((t) => `${p.title} ${p.content}`.toLowerCase().includes(t)),
      );
      return {
        content: [
          {
            type: 'text' as const,
            text: hits.length
              ? hits.map((p) => `${p.id} — ${p.title}\n${p.content}`).join('\n\n')
              : 'Aucune procédure trouvée.',
          },
        ],
      };
    },
  );
  return server;
}
