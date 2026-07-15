"""Placeholder worker entrypoint for future ML tasks."""

from __future__ import annotations


def health() -> dict[str, str]:
    return {"status": "ok", "worker": "python"}


if __name__ == "__main__":
    print(health())
