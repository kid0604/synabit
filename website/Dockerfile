# Build stage
FROM node:20-alpine AS builder

WORKDIR /app
# Install dependencies
COPY package*.json ./
RUN npm install

# Build the Nuxt app
COPY . .
RUN npm run build

# Production stage
FROM node:20-alpine

WORKDIR /app
COPY --from=builder /app/.output ./.output

# Set environment variables
ENV HOST=0.0.0.0
ENV PORT=3000
ENV NODE_ENV=production

EXPOSE 3000

CMD ["node", ".output/server/index.mjs"]
