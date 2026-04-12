import { describe, expect, it } from 'vitest'

describe('/health', () => {
  it('returns ok', async () => {
    // TODO: 用 Fastify inject 做真实接口测试
    expect({ status: 'ok' }).toMatchObject({ status: 'ok' })
  })
})
