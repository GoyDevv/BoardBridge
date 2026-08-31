# MateriaCord 2.0

Mobile-first Material 3 Discord bot client + Node backend.

## Included

- Android-first responsive Material 3 UI
- Real bot-token login with an httpOnly session cookie
- Server-side Discord REST adapter
- Discord Gateway connection with heartbeat + resume
- Live message create/update/delete events
- Live reaction-event refresh
- Server and channel/category navigation
- Text messages and multipart attachments
- Swipe-right reply gesture + Discord message references
- Double-tap configurable quick reaction
- Reaction picker, search, edit/delete for the connected bot's own messages
- Settings sheet with compact/reduced-motion/dark-mode controls
- Demo mode without Discord credentials

## Local run

`npm install && npm start` then open `http://localhost:3000`.

## Deploy

GitHub Pages can host the static UI, but the real bot backend must run on a Node host such as Render, Railway, Fly.io or another HTTPS WebSocket-capable service. The included `Dockerfile` is ready for container deployment.

Never commit a bot token. Enable only the Gateway intents and bot permissions your application actually needs.
