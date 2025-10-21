SHELL := /bin/bash
COMPOSE := docker compose -f ops/docker-compose.yml

.PHONY: bootstrap migrate up down logs psql tail build-images health

bootstrap:
	@echo "Using Node version from .nvmrc"; nvm use || true
	@echo "Rust toolchain:"; rustup show
	@echo "Docker version:"; docker --version

build-images:
	docker build -f ops/Dockerfile.api -t explorer-api:dev .
	docker build -f ops/Dockerfile.ingestor -t explorer-ingestor:dev .
	docker build -f ops/Dockerfile.web -t explorer-web:dev .

migrate:
	@echo "Applying sqlx migrations..."
	DATABASE_URL=$${DATABASE_URL:-"postgres://explorer:explorer@localhost:5432/explorer"} sqlx migrate run --source db/migrations
	@echo "Done."

up: build-images
	$(COMPOSE) up -d

down:
	$(COMPOSE) down -v

logs:
	$(COMPOSE) logs --tail=200

tail:
	$(COMPOSE) logs -f

psql:
	docker exec -it $$(docker ps --filter name=postgres --format '{{.ID}}') psql -U explorer -d explorer

health:
	$(COMPOSE) ps --format 'table {{.Service}}\t{{.State}}\t{{.Health}}'
