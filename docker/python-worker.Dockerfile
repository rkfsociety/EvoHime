FROM python:3.12-slim

WORKDIR /workspace
COPY workers/python /workspace/workers/python

EXPOSE 8090
CMD ["python", "workers/python/worker.py", "--host", "0.0.0.0", "--port", "8090"]
