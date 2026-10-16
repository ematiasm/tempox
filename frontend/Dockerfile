FROM node:24-bookworm-slim AS deps
WORKDIR /app
# The lockfile is copied with the manifest, and `npm ci` installs from it
# rather than resolving the ranges again. Without this the image could build a
# different dependency tree from the one CI validated; `npm ci` also fails
# loudly when the two disagree instead of quietly picking newer versions.
#
# Every package here is a devDependency, `vite dev` included, so no
# `--omit=dev`: SvelteKit builds to a compiled server and ships no runtime
# dependencies.
COPY package.json package-lock.json ./
RUN npm ci --no-audit --no-fund

FROM node:24-bookworm-slim
WORKDIR /app
COPY --from=deps /app/node_modules ./node_modules
# `COPY . .` is safe only because .dockerignore keeps generated artifacts out:
# without it this line overwrites the install above with the host's tree.
COPY . .
ENV HOST=0.0.0.0
EXPOSE 5173
CMD ["npx", "vite", "dev", "--host", "0.0.0.0", "--port", "5173"]
