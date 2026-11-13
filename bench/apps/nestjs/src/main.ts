// NestJS (controller + injectable service + module), node-postgres for data
// access. PLATFORM=express (Nest's default) or PLATFORM=fastify. One process
// per CPU (node:cluster), like the Fastify contender.
import 'reflect-metadata';
import cluster from 'node:cluster';
import {
  Body, Controller, Get, Injectable, Module, NotFoundException, OnModuleDestroy,
  Param, ParseIntPipe, Post, Query,
} from '@nestjs/common';
import { NestFactory } from '@nestjs/core';
import { FastifyAdapter } from '@nestjs/platform-fastify';
import { Pool } from 'pg';

const workers = Number(process.env.WORKERS || 1);

interface User { id: number; name: string; email: string; age: number }
class CreateUserDto { name!: string; email!: string; age!: number }

const toUser = (r: any): User => ({ id: Number(r.id), name: r.name, email: r.email, age: Number(r.age) });

@Injectable()
class UsersService implements OnModuleDestroy {
  private readonly pool = new Pool({
    connectionString: process.env.DATABASE_URL,
    max: Math.max(1, Math.floor(Number(process.env.DB_POOL || 32) / workers)),
  });

  async findOne(id: number): Promise<User | null> {
    const { rows } = await this.pool.query({ name: 'get', text: 'SELECT id, name, email, age FROM users WHERE id = $1', values: [id] });
    return rows.length ? toUser(rows[0]) : null;
  }

  async list(limit: number, offset: number): Promise<User[]> {
    const { rows } = await this.pool.query({ name: 'list', text: 'SELECT id, name, email, age FROM users ORDER BY id LIMIT $1 OFFSET $2', values: [limit, offset] });
    return rows.map(toUser);
  }

  async create(dto: CreateUserDto): Promise<User> {
    const { rows } = await this.pool.query({ name: 'create', text: 'INSERT INTO users (name, email, age) VALUES ($1, $2, $3) RETURNING id, name, email, age', values: [dto.name, dto.email, dto.age] });
    return toUser(rows[0]);
  }

  onModuleDestroy() { return this.pool.end(); }
}

@Controller()
class UsersController {
  constructor(private readonly users: UsersService) {}

  @Get('ping')
  ping() { return { ok: true }; }

  @Get('users/:id')
  async get(@Param('id', ParseIntPipe) id: number) {
    const user = await this.users.findOne(id);
    if (!user) throw new NotFoundException();
    return { user };
  }

  @Get('users')
  async list(@Query('limit') limit?: string, @Query('offset') offset?: string) {
    const l = Math.min(Math.max(Number(limit ?? 100), 0), 1000);
    const o = Math.max(Number(offset ?? 0), 0);
    return { users: await this.users.list(l, o) };
  }

  @Post('users')
  async create(@Body() dto: CreateUserDto) {
    return { user: await this.users.create(dto) };
  }
}

@Module({ controllers: [UsersController], providers: [UsersService] })
class AppModule {}

async function bootstrap() {
  const port = Number(process.env.APP_PORT || 8080);
  const app = process.env.PLATFORM === 'fastify'
    ? await NestFactory.create(AppModule, new FastifyAdapter(), { logger: ['error'] })
    : await NestFactory.create(AppModule, { logger: ['error'] });
  await app.listen(port, '0.0.0.0');
}

if (cluster.isPrimary && workers > 1) {
  for (let i = 0; i < workers; i++) cluster.fork();
} else {
  bootstrap();
}
