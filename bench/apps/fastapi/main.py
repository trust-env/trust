"""FastAPI + asyncpg, served by uvicorn (uvloop + httptools), one worker per CPU."""
import os

import asyncpg
from fastapi import FastAPI, HTTPException
from fastapi.responses import ORJSONResponse
from pydantic import BaseModel

WORKERS = int(os.environ.get("WORKERS", "1"))
app = FastAPI(default_response_class=ORJSONResponse)
pool: asyncpg.Pool | None = None


class NewUser(BaseModel):
    name: str
    email: str
    age: int


@app.on_event("startup")
async def startup() -> None:
    global pool
    size = max(1, int(os.environ.get("DB_POOL", "32")) // WORKERS)
    pool = await asyncpg.create_pool(os.environ["DATABASE_URL"], min_size=1, max_size=size)


@app.get("/ping")
async def ping():
    return {"ok": True}


@app.get("/users/{user_id}")
async def get_user(user_id: int):
    row = await pool.fetchrow("SELECT id, name, email, age FROM users WHERE id = $1", user_id)
    if row is None:
        raise HTTPException(status_code=404)
    return {"user": dict(row)}


@app.get("/users")
async def list_users(limit: int = 100, offset: int = 0):
    rows = await pool.fetch(
        "SELECT id, name, email, age FROM users ORDER BY id LIMIT $1 OFFSET $2",
        min(max(limit, 0), 1000), max(offset, 0),
    )
    return {"users": [dict(r) for r in rows]}


@app.post("/users")
async def create_user(user: NewUser):
    row = await pool.fetchrow(
        "INSERT INTO users (name, email, age) VALUES ($1, $2, $3) RETURNING id, name, email, age",
        user.name, user.email, user.age,
    )
    return {"user": dict(row)}
