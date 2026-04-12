FROM node:24-alpine

RUN corepack enable && corepack prepare pnpm@10.12.4 --activate

WORKDIR /app

COPY pnpm-workspace.yaml package.json pnpm-lock.yaml ./
COPY packages/server/package.json ./packages/server/
COPY packages/web/package.json ./packages/web/
COPY packages/shared/package.json ./packages/shared/

RUN pnpm install --frozen-lockfile

COPY packages/server/ ./packages/server/
COPY packages/web/ ./packages/web/
COPY packages/shared/ ./packages/shared/

RUN pnpm --filter @chatbot/web build && pnpm --filter @chatbot/server build

EXPOSE 3000

CMD ["node", "packages/server/dist/index.js"]
