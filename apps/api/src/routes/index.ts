import type { FastifyInstance } from 'fastify';
import type { AppServices } from '../services';
import { registerAdminRoutes } from './admin';
import { registerApprovalRoutes } from './approvals';
import { registerAuthRoutes } from './auth';
import { registerConversationRoutes } from './conversations';
import { registerMemoryRoutes } from './memory';
import { registerRunEventRoutes } from './run-events';
import { registerRunRoutes } from './runs';

export function registerRoutes(app: FastifyInstance, services: AppServices): void {
  registerAuthRoutes(app, services);
  registerConversationRoutes(app, services);
  registerRunRoutes(app, services);
  registerRunEventRoutes(app, services);
  registerApprovalRoutes(app, services);
  registerMemoryRoutes(app, services);
  registerAdminRoutes(app, services);
}
