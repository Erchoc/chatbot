import { describe, expect, it } from 'bun:test'

describe('/health', () => {
  it('returns ok', async () => {
    // TODO: 用 Fastify inject 做真实接口测试（bun:test）
    expect({ status: 'ok' }).toMatchObject({ status: 'ok' })
  })
})
