// The waitlist: an email address, added to one set in Upstash Redis, which
// the Vercel Marketplace integration provides (its REST address and token in
// the environment). Nothing else about the person is kept — no name, no
// address, no device. Without the integration the waitlist says it is not
// open, rather than pretending to take the address.

const url = process.env.KV_REST_API_URL ?? process.env.UPSTASH_REDIS_REST_URL
const token = process.env.KV_REST_API_TOKEN ?? process.env.UPSTASH_REDIS_REST_TOKEN

const EMAIL = /^[^\s@]+@[^\s@]+\.[^\s@]+$/

export async function POST(request: Request) {
  if (!url || !token) return Response.json({ error: 'not-open' }, { status: 503 })

  let email: unknown
  try {
    ;({ email } = await request.json())
  } catch {
    return Response.json({ error: 'bad-request' }, { status: 400 })
  }
  if (typeof email !== 'string' || email.length > 254 || !EMAIL.test(email.trim())) {
    return Response.json({ error: 'bad-email' }, { status: 400 })
  }

  // Upstash's REST interface takes a Redis command as a JSON array.
  const r = await fetch(url, {
    method: 'POST',
    headers: { authorization: `Bearer ${token}` },
    body: JSON.stringify(['SADD', 'waitlist', email.trim().toLowerCase()])
  })
  if (!r.ok) return Response.json({ error: 'store' }, { status: 502 })
  return Response.json({ ok: true })
}
