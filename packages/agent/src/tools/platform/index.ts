/** Built-in platform tools available to every agent run (memory and skills). */
import type { AnyToolDefinition } from '@mimorya/core';
import { createMemoryTools, type MemoryToolDeps } from './memory-tools';
import { createSkillTools } from './skill-tools';
import type { SkillStore } from '../../runtime/ports';

export interface PlatformToolDeps extends MemoryToolDeps {
  skills: SkillStore;
}

export function createPlatformTools(deps: PlatformToolDeps): AnyToolDefinition[] {
  return [...createMemoryTools(deps), ...createSkillTools(deps)];
}
