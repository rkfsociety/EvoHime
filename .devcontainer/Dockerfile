FROM mcr.microsoft.com/devcontainers/rust:1-bookworm

RUN apt-get update \
    && apt-get install -y --no-install-recommends nodejs npm python3 python3-pip ripgrep \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /workspaces/EvoHime
