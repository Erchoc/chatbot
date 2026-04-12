import { randomUUID } from 'node:crypto'
import Fastify from 'fastify'

const PORT = Number(process.env.PORT ?? 7758)
const ENV = process.env.NODE_ENV ?? 'production'

function ts() {
  return new Date().toISOString()
}
function level(sc: number) {
  if (sc >= 500) return '\x1b[31m[ERROR]\x1b[0m'
  if (sc >= 400) return '\x1b[33m[WARN] \x1b[0m'
  return '\x1b[32m[INFO] \x1b[0m'
}

const app = Fastify({ logger: false, genReqId: () => randomUUID() })

app.addHook('onSend', async (req, reply) => {
  reply.header('x-trace-id', req.id as string)
  const sc = reply.statusCode
  console.log(`${level(sc)} [${ts()}] ${req.method} ${req.url} → ${sc} | trace:${req.id}`)
})

app.get('/health', async () => ({ status: 'ok', ts: ts() }))

const start = async () => {
  await app.listen({ port: PORT, host: '0.0.0.0' })
  console.log(`\n  chatbot · ${ENV} · http://localhost:${PORT}\n`)
}
start()
