# chatbot

> 模式：SPA（Vite + React + Http Service）

## 快速启动

```bash
pnpm install
pnpm dev        # server :7758 + web :3000 并行启动
```

## 测试

```bash
pnpm test          # web + server 全量
pnpm test:unit     # 仅单元测试
```

## 发布前自检

```bash
pnpm verify        # lint + typecheck + test + build
```

## 部署

```bash
pnpm ship          # 部署到 Fly/Runway/Vercel
```

## 目录说明

| 目录 | 说明 |
|------|------|
| packages/web | 前端（Vite + React）|
| packages/server | 后端 + 静态文件托管 |

## 验收标准

- [ ] `pnpm install && pnpm build` 无报错
- [ ] `pnpm test` 全部通过（web + server）
- [ ] `pnpm lint` 无错误
- [ ] `pnpm typecheck` 无错误
- [ ] `GET /health` 返回 200
- [ ] `pnpm ship` 部署成功

