import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js';
import { createDemoDocsServer } from './demo-docs-server';

await createDemoDocsServer().connect(new StdioServerTransport());
