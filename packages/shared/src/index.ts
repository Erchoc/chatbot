import { randomUUID } from 'node:crypto'

export function ts(): string {
  return new Date().toISOString()
}

export function randomId(): string {
  return randomUUID()
}
