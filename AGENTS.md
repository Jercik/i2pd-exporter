# Rule: Read the Startup Files First

Before taking any action, read @README.md.

# Rule: Execute Commands from Arrays, Not Strings

## Store commands in arrays, not strings

When Bash expands a string variable, quotes inside become literal characters and whitespace triggers word splitting:

```bash
# BAD
CMD="echo \"hello world\""
$CMD  # outputs: "hello world" (with literal quotes)

# GOOD: array preserves argument boundaries
CMD=(echo "hello world")
"${CMD[@]}"  # outputs: hello world
```

## Never interpolate variables into shell strings

Variables interpolated into shell strings — `sh -c`, `bash -c`, `eval`, `ssh host` — are reparsed by the shell. Characters like `$(...)`, backticks, or `;` in the value execute as code — an injection vector:

```bash
# BAD: if VAR contains $(malicious), it executes
sh -c "$VAR --write"

# GOOD: direct execution, no shell interpretation
"${CMD[@]}" --write

# GOOD: with xargs, execute the array directly
find . -name '*.js' -print0 | xargs -0 "${CMD[@]}" --write --
```

When you need shell features (pipes, redirects), use the `exec "$@"` pattern to pass arguments as positional parameters instead of interpolating them:

```bash
# GOOD: arguments passed as $@, not interpolated into the string; the redirect is why sh -c is needed
find . -name '*.js' -print0 | xargs -0 sh -c 'exec "$@" >> format.log 2>&1' _ "${CMD[@]}" --write --
```

The `_` occupies `$0` (the script name), leaving `$@` for the command and arguments.

# Rule: Prefer Debian Slim Over Alpine Base Images

Use `-slim` Debian variants (e.g. `node:26-bookworm-slim`, `python:3.12-slim-bookworm`) as container base images — Alpine's musl libc breaks glibc prebuilt binaries and forces native-module rebuilds, and the base-image size savings become marginal once app dependencies land.

# Rule: Set the Container UID in the Orchestrator, Not the Image

Do not create users inside container images. Let the orchestrator (Podman Quadlet, Kubernetes, docker-compose) specify the non-root UID and GID the container runs as. A hardcoded `USER` couples the image to one UID: clusters enforcing Pod Security Standards or OPA Gatekeeper can require arbitrary UIDs, and an orchestrator-assigned UID that differs from the image's causes volume-ownership conflicts Podman papers over with the non-portable `:U` volume flag.

```dockerfile
# BAD
RUN groupadd -r myapp && useradd -r -g myapp myapp
USER myapp
```

Write the runtime stage without a `USER` instruction.

## Orchestrator configuration

Podman Quadlet:

```ini
[Container]
User=1100
Group=1100
Volume=/var/lib/myapp:/data:rw
```

Kubernetes sets the same thing through the pod-level `securityContext` (`runAsUser`, `runAsGroup`, `runAsNonRoot`, `fsGroup`); docker-compose through `user: "1100:1100"`.

On the host, create the user and group with the same deterministic UID/GID the orchestrator specifies (`ansible.builtin.user`/`group` with explicit `uid:`/`gid:`), and give bind-mounted data directories that owner.

# Rule: Avoid Leaky Abstractions

Design interfaces around what callers need, not how the system works internally. An abstraction is leaky when using it correctly requires knowledge of underlying storage, infrastructure, or error behavior — a connection string in a method signature, a transaction handle in a return type, an error that only makes sense one layer down, or two similar-looking methods where one reads memory and the other crosses the network. Keep signatures consistent, return domain types instead of backend artifacts, and inject infrastructure dependencies through constructors rather than method parameters.

```ts
// Leaky: exposes database concerns, inconsistent signatures
interface ReservationRepository {
  create(restaurantId: number, reservation: Reservation): number; // returns DB ID
  findById(id: string): Reservation | null; // why no restaurantId?
  update(reservation: Reservation): void;
  connect(connectionString: string): void;
}
```

```ts
// Better: consistent interface, infrastructure hidden, injected via constructor
interface ReservationRepository {
  create(restaurantId: number, draft: NewReservation): Promise<Reservation>;
  findById(restaurantId: number, id: string): Promise<Reservation | null>;
  update(restaurantId: number, reservation: Reservation): Promise<void>;
}
```

# Rule: Build for Requirements That Exist Today

Implement what the current requirement needs, nothing more. Speculative surface must be maintained, tested, and reasoned about until someone deletes it — and because the code that carries it references it, no unused-code tool will ever flag it; only authoring discipline stops it.

- No defensive handling for states the types already exclude: the null-check on a non-nullable value, the `catch` around code that cannot throw. Exhaustiveness guards (`assertNever`, `satisfies never`) are the opposite shape — they make an impossible state fail loudly instead of flowing on — and they stay.
- No parameter or option no caller passes — including one a default keeps compiling, like a `{ retries = 3 }` read in the body that every call site leaves at 3. A published CLI or library's callers are external: its documented public interface is a current requirement, never speculative surface.
- No abstraction justified only by a hypothetical second use: an interface with one implementation that hides nothing, a registry with one entry, indirection added "for flexibility". Extracting a single-use pure function into the functional core is not this — the extraction pays now, in testability, and the functional-core and file-naming rules ask for it.
- No generality justified only by a future requirement — when the requirement arrives, designing for the real case beats having guessed. The one inversion is a format that locks at its first real reader (a wire format, a stored blob, a published API shape): a locked, versionless format is the one guess that cannot be cheaply corrected, so design its evolution path — a `version` field — up front.

# Rule: Comments Explain Why, Not What

Default to writing no comments. Add one only to capture what the code cannot show — a hidden constraint, a subtle invariant, why a decision was made, which alternatives were rejected, what external factor forced a workaround — the context that stops the next person from "cleaning up" something load-bearing.

Never explain what the code does. Names convey purpose, types convey shape, the code itself conveys behavior. Never reference the current task, fix, or callers ("used by X", "added for the Y flow", "handles the case from issue #123") — those belong in the PR description and rot as the codebase evolves.

Keep the comments you write — docstrings included — to one short line; an example snippet already living in a docstring is documentation to keep type-checking, not a comment to trim.

```ts
// BAD: references caller context that will rot
// Used by the checkout flow after the Stripe webhook fires
function markOrderPaid(orderId: string): void {
  /* ... */
}

// GOOD: records a non-obvious external constraint
// Stripe caps statement descriptors at 22 chars
const statementDescriptor = raw.slice(0, 22);
```

# Rule: Prefer Deep Modules

A module earns its place by what it hides behind an interface smaller than the implementation it covers: a decision, a side effect, a detail callers no longer carry. Judge every extraction and every layer by that ratio of interface to implementation. The deletion test settles close calls: if removing the module would scatter the same knowledge across its callers, it is earning its keep; if the complexity would simply vanish, it is a pass-through.

- Never split a function or file because it is long; split for what the split hides or separates — a decision the caller need not know, a side effect kept out of the functional core.
- If understanding a caller requires repeatedly reading a callee's body, that boundary hides nothing: inline it or redesign the interface.
- A wrapper that only forwards calls adds surface without hiding anything; use the wrapped thing directly. An interface that hides infrastructure behind domain-shaped methods is the opposite case — that is depth.
- In production code, one general operation serving all current callers beats several near-duplicate special-case ones.

# Rule: Design Contracts Twice

The shape that ships first is usually just the first one that worked, and some shapes are expensive to revisit once anything depends on them: a CLI surface, a stored or wire format, JSON output that automation parses, a package's public API. Before committing to such a contract, sketch a second, meaningfully different shape and compare the two on what each asks of callers: what they must know to use it correctly, and what they can get wrong silently. Prefer the shape that keeps required knowledge small while misuse stays loud — still failing a compile, a parse, or a run. Record the winner and the loser in a sentence or two where rejected alternatives already belong — the pull request description, or a one-line comment when the code alone would not explain the choice.

Skip the sketch when the shape is dictated rather than chosen — a schema mirroring a format some other producer defines, or a surface an existing convention already fixes.

# Rule: File Naming Matches Contents

Name files for what the module does: kebab-case, verb-noun or domain-role names, matching the primary export — `calculateUsageRate` goes in `calculate-usage-rate.ts`.

## Checklist

- One responsibility per file; if the name needs two verbs, split it.
- Align with functional core/imperative shell conventions:
  - Functional core: `calculate-…`, `validate-…`, `parse-…`, `format-…`, `aggregate-…`
  - Imperative shell: `…-route.ts`, `…-handler.ts`, `…-job.ts`, `…-cli.ts`, `…-script.ts`
- Prefer specific domain nouns; avoid generic bucket file names like `utils`, `helpers`, `core`, `data`, `math`.
- Use role suffixes (`-service`, `-repository`) only when they clarify architecture.

Example: A file named `usage.core.ts` containing both fetching and aggregation logic should be split into `fetch-service-usage.ts` and `aggregate-usage.ts`.

# Rule: Separate the Functional Core from the Imperative Shell

Separate business logic from side effects by organizing code into a functional core and an imperative shell. The functional core contains pure functions that operate only on provided data, free of I/O, database calls, or state mutations. The imperative shell handles all side effects and orchestrates the core.

The payoff: the shell can change — a different database, queue, or framework — without touching business rules, and core functions work in any context. When unsure where a function belongs, ask what its test would need: a mock, a database, or a clock means shell; plain values mean core.

**Functional core:** filtering, mapping, calculations, validation, parsing, formatting, business rule evaluation.

**Imperative shell:** HTTP handlers, database queries, file I/O, API calls, message queue operations, CLI entry points.

```ts
// BAD: Logic and side effects mixed
function sendUserExpiryEmail(): void {
  for (const user of db.getUsers()) {
    if (user.subscriptionEndDate > new Date()) continue;
    if (user.isFreeTrial) continue;
    email.send(user.email, `Your account has expired, ${user.name}.`);
  }
}

// GOOD: Functional core (pure, testable)
function getExpiredUsers(users: User[], cutoff: Date): User[] {
  return users.filter((user) => user.subscriptionEndDate <= cutoff && !user.isFreeTrial);
}

function generateExpiryEmails(users: User[]): Array<[string, string]> {
  return users.map((user) => [user.email, `Your account has expired, ${user.name}.`]);
}

// Imperative shell (orchestrates side effects)
email.bulkSend(generateExpiryEmails(getExpiredUsers(db.getUsers(), new Date())));
```

Test the functional core, not the shell. Core tests are fast, deterministic, and need no mocks; the shell becomes thin orchestration where bugs are easy to spot through review. If shell tests are requested, prefer integration tests over unit tests with mocks.

# Rule: No Logic in Tests

Write test assertions as concrete input/output examples, not computed values — unlike production code that handles varied inputs, tests verify specific cases. Avoid operators, string concatenation, loops, and conditionals in test bodies — these obscure bugs.

```ts
const baseUrl = "http://example.com/";

// BAD: computed expectation hides bugs when test and production share the same error
expect(getPhotosUrl()).toBe(baseUrl + "/photos"); // passes despite double-slash bug

// GOOD: literal expected value catches the bug immediately
expect(getPhotosUrl()).toBe("http://example.com/photos"); // fails, reveals the issue
```

Use test utilities for setup and data preparation — fixtures, builders, factories, mock configuration — but never for computing expected values.

# Rule: Parse, Don't Validate

When checking input data, return a refined type that preserves the knowledge gained — don't just validate and discard. Validation functions that return `void` or a bare `boolean` force callers to re-check conditions or handle "impossible" cases the compiler could rule out — and a check whose result nothing consumes is easy to forget entirely.

Zod embodies this principle: every schema is a parser from `unknown` input to a typed output. Use it at system boundaries to convert external input — JSON, environment variables, API responses — into domain types early.

```ts
import * as z from "zod";

// Schema defines both validation rules AND the resulting type
const User = z.object({
  id: z.string(),
  email: z.email(),
  roles: z.array(z.string()).min(1),
});

type User = z.infer<typeof User>;

// Parse at the boundary — downstream code receives typed data
function handleRequest(body: unknown): User {
  return User.parse(body); // throws ZodError if invalid
}
```

- **Strengthen argument types.** Instead of accepting `T | undefined`, require callers to provide already-parsed data.
- **Let schemas encode constraints.** If a function needs a non-empty array, positive number, or valid email, define a schema that guarantees it.

# Rule: Test What Matters

Write tests where failure is expensive and the test can stay stable: business rules, public contracts, data transformations, bug regressions, and a thin set of end-to-end flows. Write fewer for pass-through forwarding, private helpers already covered through a public caller, call-count and call-order choreography, wholesale snapshots, and any test whose assertions mirror the implementation instead of the promised behavior.

```ts
// BAD: asserts internal choreography, not the promised behavior
it("saves the order via the repository", async () => {
  const repo = { save: vi.fn() };
  await createOrder({ repo }, { sku: "A1", qty: 2 });
  expect(repo.save).toHaveBeenCalledTimes(1);
});

// GOOD: asserts the business rule — what the caller was promised
it("applies the bulk discount at qty 10", () => {
  expect(totalFor([{ sku: "A1", qty: 10 }])).toBe(85);
});
```

A good test survives a behavior-preserving refactor; one that must change with it is pinned to the wrong thing.

# Rule: Use `repoq` for Repository Queries

Use `repoq` for reading repository state instead of piping `git` or the forge CLI through `awk`/`jq`/`grep`. Each command handles edge cases (detached HEAD, unborn branches, missing auth) and, under `--json`, returns validated JSON. It also carries one write verb: `pr create` opens a pull request on the detected forge, taking the body from a file or stdin rather than an argument the shell would expand. Use raw `git` for commit/push/merge, and the repo's forge CLI for the other forge-side mutations (issues, releases, PR edits) — `gh` for GitHub or `fgj` for Forgejo, per the detected provider. Run `npx -y repoq@latest --help` if unsure of the available subcommands; the explicit tag prevents `npx` from reusing a stale cached release.

# Rule: Upgrade Dependencies with `cargo update`

Run `cargo update` to upgrade dependencies to the latest versions allowed by existing SemVer ranges; this modifies `Cargo.lock` only. Edit `Cargo.toml` only to change the range itself, such as bumping `serde = "1.0"` to `serde = "2.0"`.

For `0.x` versions, Cargo treats minor bumps as breaking — `"0.12"` reaches `0.12.x`, never `0.13.0`.
