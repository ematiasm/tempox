# The frontend image, built to serve.
#
# The previous version ran `vite dev` inside the container, which is the
# development tool: it compiled on the fly, served `src/` raw (measured:
# `GET /src/lib/productForm.ts` answered 200), and carried every devDependency
# while doing it. That is the right command for an edit loop and the wrong one for
# a delivered artifact.
#
# So this is a two-stage build. The first has the toolchain and produces `build/`;
# the second has Node and that directory, and nothing else.

# ── Build stage ──────────────────────────────────────────────────────
FROM node:26-bookworm-slim AS builder
WORKDIR /app

# The lockfile is copied with the manifest, and `npm ci` installs from it rather
# than resolving the ranges again. Without this the image could build a different
# dependency tree from the one CI validated; `npm ci` also fails loudly when the
# two disagree instead of quietly picking newer versions.
COPY package.json package-lock.json ./
RUN npm ci --no-audit --no-fund

# `COPY . .` is safe only because .dockerignore keeps generated artifacts and the
# test suite out: without it this line overwrites the install above with the
# host's tree, and `tests/` would enter the build context for no reason.
COPY . .

# `adapter-node` writes `build/`. No `PUBLIC_API_BASE` is needed any more: the app
# asks its own origin and the server reads `BACKEND_URL` per request at runtime,
# so nothing about a deployment is compiled in.
RUN npm run build

# ── Runtime stage ────────────────────────────────────────────────────
FROM node:26-bookworm-slim
WORKDIR /app

# Measured before writing this: the built server imports only Node builtins
# (`node:http`, `node:fs`, `node:stream`, …) and `build/client` carries the fonts
# and assets the browser asks for. So no `node_modules` is copied at all — not
# even `--omit=dev` production dependencies. The image is `build/` plus Node.
COPY --from=builder /app/build ./build

ENV NODE_ENV=production
# `adapter-node` reads `PORT`, defaulting to 3000. Fixed to 5173 so CI, the docs
# and the compose mapping keep speaking one port.
ENV PORT=5173
EXPOSE 5173

# Not `npm start`: there is no package.json in this stage, and a dependency on the
# project's scripts would be one more thing that could differ from what was built.
CMD ["node", "build/index.js"]
