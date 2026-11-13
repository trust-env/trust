FROM python:3.12-slim
WORKDIR /app
RUN pip install --no-cache-dir fastapi==0.115.6 "uvicorn[standard]==0.34.0" asyncpg==0.30.0 orjson==3.10.12
COPY main.py .
EXPOSE 8080
CMD ["sh", "-c", "exec uvicorn main:app --host 0.0.0.0 --port ${APP_PORT:-8080} --workers ${WORKERS:-1} --no-access-log --log-level warning"]
