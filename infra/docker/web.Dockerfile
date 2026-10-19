# The web application, as Next.js's self-contained server (`standalone`
# output), run as an unprivileged user (ADR-032). Its relay of /api/* is
# set at build time: API_URL is the API as the web container reaches it.
#   docker build -f infra/docker/web.Dockerfile --build-arg API_URL=http://api:4000 -t mimorya-web .

FROM node:22-bookworm-slim AS build
RUN corepack enable
WORKDIR /src
COPY package.json pnpm-lock.yaml pnpm-workspace.yaml tsconfig.base.json ./
COPY apps/web/package.json apps/web/
RUN pnpm install --frozen-lockfile
COPY apps/web apps/web
ARG API_URL=http://api:4000
# `true` lists the fictional demo accounts on the sign-in page.
ARG NEXT_PUBLIC_DEMO_MODE=false
ENV API_URL=$API_URL NEXT_PUBLIC_DEMO_MODE=$NEXT_PUBLIC_DEMO_MODE NEXT_OUTPUT=standalone \
    NEXT_TELEMETRY_DISABLED=1
RUN pnpm --filter @mimorya/web build

FROM node:22-bookworm-slim
WORKDIR /app
ENV NODE_ENV=production NEXT_TELEMETRY_DISABLED=1 HOSTNAME=0.0.0.0 PORT=3000
COPY --from=build --chown=node:node /src/apps/web/.next/standalone ./
COPY --from=build --chown=node:node /src/apps/web/.next/static ./apps/web/.next/static
USER node
EXPOSE 3000
CMD ["node", "apps/web/server.js"]
