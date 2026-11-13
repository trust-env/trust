// Fastify + node-postgres. One process per CPU (node:cluster) so Node gets
// the same cores as the multi-threaded runtimes.
const cluster = require('node:cluster');
const workers = Number(process.env.WORKERS || 1);

if (cluster.isPrimary && workers > 1) {
  for (let i = 0; i < workers; i++) cluster.fork();
} else {
  const Fastify = require('fastify');
  const { Pool } = require('pg');
  const pool = new Pool({
    connectionString: process.env.DATABASE_URL,
    max: Math.max(1, Math.floor(Number(process.env.DB_POOL || 32) / workers)),
  });
  const app = Fastify({ logger: false });

  const userSchema = {
    type: 'object',
    properties: { id: { type: 'integer' }, name: { type: 'string' }, email: { type: 'string' }, age: { type: 'integer' } },
  };
  const toUser = (r) => ({ id: Number(r.id), name: r.name, email: r.email, age: Number(r.age) });

  app.get('/ping', async () => ({ ok: true }));

  app.get('/users/:id', {
    schema: { response: { 200: { type: 'object', properties: { user: userSchema } } } },
  }, async (req, reply) => {
    const { rows } = await pool.query({ name: 'get', text: 'SELECT id, name, email, age FROM users WHERE id = $1', values: [req.params.id] });
    if (!rows.length) return reply.code(404).send();
    return { user: toUser(rows[0]) };
  });

  app.get('/users', {
    schema: { response: { 200: { type: 'object', properties: { users: { type: 'array', items: userSchema } } } } },
  }, async (req) => {
    const limit = Math.min(Math.max(Number(req.query.limit ?? 100), 0), 1000);
    const offset = Math.max(Number(req.query.offset ?? 0), 0);
    const { rows } = await pool.query({ name: 'list', text: 'SELECT id, name, email, age FROM users ORDER BY id LIMIT $1 OFFSET $2', values: [limit, offset] });
    return { users: rows.map(toUser) };
  });

  app.post('/users', {
    schema: {
      body: { type: 'object', required: ['name', 'email', 'age'], properties: { name: { type: 'string' }, email: { type: 'string' }, age: { type: 'integer' } } },
      response: { 200: { type: 'object', properties: { user: userSchema } } },
    },
  }, async (req) => {
    const { name, email, age } = req.body;
    const { rows } = await pool.query({ name: 'create', text: 'INSERT INTO users (name, email, age) VALUES ($1, $2, $3) RETURNING id, name, email, age', values: [name, email, age] });
    return { user: toUser(rows[0]) };
  });

  app.listen({ host: '0.0.0.0', port: Number(process.env.APP_PORT || 8080) });
}
