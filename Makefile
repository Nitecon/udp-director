.PHONY: help build test fmt lint clean docker-build docker-push k8s-deploy k8s-delete

help:
	@echo "UDP Director - Makefile Commands"
	@echo ""
	@echo "Development:"
	@echo "  make build        - Build the project in release mode"
	@echo "  make test         - Run all tests"
	@echo "  make fmt          - Format code with rustfmt"
	@echo "  make lint         - Run clippy linter"
	@echo "  make clean        - Clean build artifacts"
	@echo ""
	@echo "Docker:"
	@echo "  make docker-build - Build Docker image locally"
	@echo "  make docker-push  - Build and push to registry.nitecon.net"
	@echo ""
	@echo "Kubernetes:"
	@echo "  make k8s-deploy   - Deploy to Kubernetes"
	@echo "  make k8s-delete   - Remove from Kubernetes"

build:
	cargo build --release

test:
	cargo test

fmt:
	cargo fmt

lint:
	cargo clippy -- -D warnings

clean:
	cargo clean

docker-build:
	docker build -t udp-director:latest .

docker-push:
	./dockerpush.sh

k8s-deploy:
	kubectl apply -f k8s/rbac.yaml
	kubectl apply -f k8s/configmap.yaml
	kubectl apply -f k8s/deployment.yaml

k8s-delete:
	kubectl delete -f k8s/deployment.yaml
	kubectl delete -f k8s/configmap.yaml
	kubectl delete -f k8s/rbac.yaml
