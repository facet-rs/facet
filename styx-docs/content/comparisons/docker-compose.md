+++
title = "Docker Compose"
weight = 4
slug = "docker-compose"
insert_anchor_links = "heading"
+++

A Docker Compose file in YAML vs Styx.

```compare
/// yaml
version: "3.8"

services:
  web:
    build:
      context: .
      dockerfile: Dockerfile
      args:
        - NODE_ENV=production
    ports:
      - "3000:3000"
    environment:
      - DATABASE_URL=postgres://db:5432/myapp
      - REDIS_URL=redis://cache:6379
    depends_on:
      - db
      - cache
    volumes:
      - ./uploads:/app/uploads
    networks:
      - frontend
      - backend
    restart: unless-stopped
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:3000/health"]
      interval: 30s
      timeout: 10s
      retries: 3

  db:
    image: postgres:18-alpine
    environment:
      POSTGRES_DB: myapp
      POSTGRES_USER: myapp
      POSTGRES_PASSWORD_FILE: /run/secrets/db_password
    volumes:
      - postgres_data:/var/lib/postgresql/data
    networks:
      - backend
    secrets:
      - db_password

  cache:
    image: redis:7-alpine
    command: redis-server --appendonly yes
    volumes:
      - redis_data:/data
    networks:
      - backend

volumes:
  postgres_data:
  redis_data:

networks:
  frontend:
  backend:

secrets:
  db_password:
    file: ./secrets/db_password.txt
/// styx
version 3.8

services {
  web {
    build {
      context .
      dockerfile Dockerfile
      args (NODE_ENV=production)
    }
    ports (3000:3000)
    environment (
      DATABASE_URL=postgres://db:5432/myapp
      REDIS_URL=redis://cache:6379
    )
    depends_on (db cache)
    volumes (./uploads:/app/uploads)
    networks (frontend backend)
    restart unless-stopped
    healthcheck {
      test (CMD curl -f http://localhost:3000/health)
      interval 30s
      timeout 10s
      retries 3
    }
  }

  db {
    image postgres:18-alpine
    environment {
      POSTGRES_DB myapp
      POSTGRES_USER myapp
      POSTGRES_PASSWORD_FILE /run/secrets/db_password
    }
    volumes (postgres_data:/var/lib/postgresql/data)
    networks (backend)
    secrets (db_password)
  }

  cache {
    image redis:7-alpine
    command "redis-server --appendonly yes"
    volumes (redis_data:/data)
    networks (backend)
  }
}

volumes {
  postgres_data // no value? defaults to @ (unit)
  redis_data
}

networks {
  frontend
  backend
}

secrets {
  db_password.file ./secrets/db_password.txt
}
```
