SHELL := /bin/bash
COMPOSE := docker compose -f ops/docker-compose.yml

.PHONY: bootstrap migrate up down logs psql tail build-images

bootstrap:
	@echo "Using Node version from .nvmrc"; nvm use || true
	@echo "Rust toolchain:"; rustup show
	@echo "Docker version:"; docker --version

build-images:
	docker build -f ops/Dockerfile.api -t explorer-api:dev .
	docker build -f ops/Dockerfile.ingestor -t explorer-ingestor:dev .
	docker build -f ops/Dockerfile.web -t explorer-web:dev .

migrate:
	@echo "Run DB migrations here (sqlx/diesel)."
	@echo "Placeholder: ensure postgres is up, then apply migrations."

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
