# Webhooks

Saldo POSTs every change in a household to the URLs its owner registers: a transaction booked from the phone can turn on a light in Home Assistant, feed an n8n flow or update a spreadsheet. This page is for people writing a receiver.

## Managing endpoints

Only the household owner manages webhooks, in the web app (Webhooks in the menu) or through the API:

| Request | What it does |
|---|---|
| `GET /api/v1/webhooks` | List endpoints (never shows secrets) |
| `POST /api/v1/webhooks` | Create: `{ "name", "url", "event_types"?, "include_import_events"? }`. The response holds the `secret`, **once** |
| `GET/PATCH/DELETE /api/v1/webhooks/{id}` | Read, change (`name`, `url`, `event_types`, `include_import_events`, `active`), delete |
| `POST /api/v1/webhooks/{id}/test` | Queue a `webhook.test` event that only this endpoint receives |
| `POST /api/v1/webhooks/{id}/rotate-secret` | Replace the secret; the new one is in the response, once |
| `GET /api/v1/webhooks/{id}/deliveries` | The delivery log, newest event first |
| `POST /api/v1/webhook-deliveries/{id}/redeliver` | Queue a delivery again, now, with a fresh set of attempts |

`url` must be `http` or `https`. Private addresses (`192.168.x.x`, `homeassistant.local`) are allowed on purpose.

`event_types` is a list of patterns; an event is sent if any pattern matches. Default `["*"]`.

- `*`: every event
- `transaction.*`: every type that starts with `transaction.`
- `bill.overdue`: exactly that type

Events written by the Firefly importer (origin `import`) are sent only when `include_import_events` is true, so a first import of 12,000 transactions does not flood a receiver. An endpoint receives only events that happened after it was created.

## Event catalogue

Types are `<entity>.<verb>`.

| Entity | Types | `data` holds |
|---|---|---|
| Household | `household.created`, `household.updated` | the household |
| Member | `household_member.created`, `household_member.deleted` | the member |
| Account | `account.created`, `account.updated`, `account.deleted` | the account |
| Transaction | `transaction.created`, `transaction.updated`, `transaction.deleted` | the whole group with all its splits |
| Category, tag | `category.*`, `tag.*` (`created`, `updated`, `deleted`) | the category or tag |
| Attachment | `attachment.created`, `attachment.deleted` | the attachment record (no file contents) |
| Webhook | `webhook.created`, `webhook.updated`, `webhook.deleted` | the endpoint, never its secret |
| Test | `webhook.test` | `{ "webhook": ..., "message": "Test delivery from Saldo" }` |
| Import | `import.completed` | the import summary |

Planned in later phases and delivered the same way: `budget.*`, `budget_limit.*`, `bill.*` (also `bill.due_soon`, `bill.overdue`, `bill.paid`, `bill.period_reopened`), `piggy_bank.*` (also `piggy_bank.target_reached`), `recurrence.*` (also `recurrence.fired`), `rule.*`.

For `*.deleted`, `data` is the entity's last state. `data` always has the same shape as the API's response for that entity.

## The request

```http
POST /your/path HTTP/1.1
Content-Type: application/json
User-Agent: Saldo-Webhook/0.1.0
X-Saldo-Event-Id: 0193c1a2-7b3e-7c00-8000-000000000001
X-Saldo-Event-Type: transaction.created
X-Saldo-Delivery-Id: 0193c1a2-7b3f-7c00-8000-000000000002
X-Saldo-Signature: t=1794428460,v1=7a64ecf7bcdeeda8dd77aa39fddd013ac2b879b24dfc23c8a772f83070ff68c3

{
  "id": "0193c1a2-7b3e-7c00-8000-000000000001",
  "type": "transaction.created",
  "occurred_at": "2026-11-11T20:21:00.000Z",
  "household_id": "0193...",
  "actor": { "user_id": "0193..." },
  "origin": "api",
  "data": { "id": "0193...", "title": "Groceries", "splits": [ ... ] }
}
```

`origin` is `api`, `import`, `scheduler`, `rule` or `system`. Amounts inside `data` are decimal strings with a currency code (`"amount": "12.50", "currency": "EUR"`), never JSON numbers.

## Verifying the signature

`X-Saldo-Signature` is `t=<unix seconds>,v1=<hex>` where

```
v1 = hex( HMAC-SHA256( key = secret, message = "<t>." + raw request body ) )
```

The key is the whole secret string as shown (including the `whsec_` prefix), as UTF-8 bytes. A receiver should:

1. Read the **raw** body before parsing JSON (re-encoding changes the bytes).
2. Split the header on `,` and each part on the first `=`; take `t` and every `v1` (ignore other keys, so a future `v2` does not break you).
3. Reject the request if `t` is more than 5 minutes away from your clock (replay protection).
4. Compute the HMAC and compare it with each `v1` **in constant time**. Accept if one matches.

### PHP

```php
<?php
function saldo_verify(string $secret, string $header, string $body, int $tolerance = 300): bool
{
    $t = null;
    $signatures = [];
    foreach (explode(',', $header) as $part) {
        [$key, $value] = array_pad(explode('=', trim($part), 2), 2, '');
        if ($key === 't') $t = (int) $value;
        if ($key === 'v1') $signatures[] = $value;
    }
    if ($t === null || $signatures === [] || abs(time() - $t) > $tolerance) {
        return false;
    }
    $expected = hash_hmac('sha256', $t . '.' . $body, $secret);
    foreach ($signatures as $signature) {
        if (hash_equals($expected, $signature)) return true; // constant time
    }
    return false;
}

// Laravel: saldo_verify(config('services.saldo.secret'),
//     $request->header('X-Saldo-Signature', ''), $request->getContent())
$body = file_get_contents('php://input');
if (!saldo_verify(getenv('SALDO_WEBHOOK_SECRET'), $_SERVER['HTTP_X_SALDO_SIGNATURE'] ?? '', $body)) {
    http_response_code(401);
    exit;
}
$event = json_decode($body, true);
```

### Go

```go
package saldohook

import (
	"crypto/hmac"
	"crypto/sha256"
	"encoding/hex"
	"io"
	"net/http"
	"strconv"
	"strings"
	"time"
)

// Verify checks an X-Saldo-Signature header against the raw body.
func Verify(secret, header string, body []byte, now time.Time, tolerance time.Duration) bool {
	var t int64 = -1
	var signatures [][]byte
	for _, part := range strings.Split(header, ",") {
		key, value, _ := strings.Cut(strings.TrimSpace(part), "=")
		switch key {
		case "t":
			if parsed, err := strconv.ParseInt(value, 10, 64); err == nil {
				t = parsed
			}
		case "v1":
			if raw, err := hex.DecodeString(value); err == nil {
				signatures = append(signatures, raw)
			}
		}
	}
	if t < 0 || len(signatures) == 0 {
		return false
	}
	if age := now.Sub(time.Unix(t, 0)); age > tolerance || age < -tolerance {
		return false
	}
	mac := hmac.New(sha256.New, []byte(secret))
	mac.Write([]byte(strconv.FormatInt(t, 10) + "."))
	mac.Write(body)
	expected := mac.Sum(nil)
	for _, signature := range signatures {
		if hmac.Equal(expected, signature) { // constant time
			return true
		}
	}
	return false
}

// Handler shows the check in an http.Handler.
func Handler(secret string, next func(eventID string, body []byte)) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		body, err := io.ReadAll(io.LimitReader(r.Body, 10<<20))
		if err != nil || !Verify(secret, r.Header.Get("X-Saldo-Signature"), body, time.Now(), 5*time.Minute) {
			http.Error(w, "bad signature", http.StatusUnauthorized)
			return
		}
		next(r.Header.Get("X-Saldo-Event-Id"), body)
		w.WriteHeader(http.StatusNoContent)
	}
}
```

Test vector: secret `whsec_test`, `t = 1794428460`, body `{"id":"1"}` gives `v1 = 7a64ecf7bcdeeda8dd77aa39fddd013ac2b879b24dfc23c8a772f83070ff68c3`.

## Delivery and retries

- Answer with any **2xx** within **10 seconds**. Everything else (3xx, 4xx, 5xx, timeout, refused connection) is a failure. Redirects are not followed.
- After a failure Saldo waits 1 min, 5 min, 30 min, 2 h, 6 h, 12 h, 24 h (each plus up to 10% random jitter) and tries again. After the **8th** failed attempt (about 45 hours) the delivery is `dead`; the owner can redeliver it from the delivery log.
- **20 dead deliveries in a row** deactivate the endpoint; the reason is shown on the endpoint, and a `webhook.updated` event (origin `system`) is written. Activating it again resets the count. Deliveries queued for an inactive endpoint wait until it is active again.
- Delivery is **at least once** and **not ordered**: a receiver may see the same event twice (after a crash or a redeliver) and may see an `updated` before its `created` when a retry is involved. Deduplicate on `X-Saldo-Event-Id`, and order by `occurred_at` if order matters.
- Respond quickly and do slow work afterwards; up to 4 deliveries are in flight at once.

## Live updates in the browser

Separate from webhooks, `GET /api/v1/events/stream` is a Server-Sent Events stream for signed-in household members. Each message is `id: <seq>` plus `data: {"seq", "type", "entity_type", "entity_id"}` (no entity data). Reconnecting with `Last-Event-ID` resumes after that event; `?after_seq=` does the same for clients that cannot set headers. Without either the stream starts with the next new event. A comment line is sent every 15 seconds to keep proxies from closing the connection.
