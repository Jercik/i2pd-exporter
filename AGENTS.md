# Rule: Mandatory Startup Reads

Before taking any action, read @README.md for project context.

# Rule: `askpplx` CLI Usage

Use `askpplx` for real-time web search via Perplexity. Verify external facts—documentation, API behavior, library versions, best practices—before acting on them. A lookup costs far less than debugging hallucinated code. Run `npx -y askpplx --help` if unsure of the available options.

# Rule: Safe Command Execution

## Store commands in arrays, not strings

When Bash expands a string variable, quotes inside become literal characters and whitespace triggers word splitting:

```bash
# BAD: quotes are literal, spaces split words
CMD="echo \"hello world\""
$CMD  # outputs: "hello world" (with literal quotes)

# GOOD: array preserves argument boundaries
CMD=(echo "hello world")
"${CMD[@]}"  # outputs: hello world
```

## Never interpolate variables into shell strings

Variables interpolated into shell strings — `sh -c`, `bash -c`, `eval`, `ssh host` — are reparsed by the shell. Characters like `$(...)`, backticks, or `;` in the value execute as code, a classic injection vector:

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
# GOOD: arguments passed as $@, not interpolated into the string
xargs -0 sh -c 'exec "$@"' _ "${CMD[@]}" --write --
```

The `_` occupies `$0` (the script name), leaving `$@` for the command and arguments. Any string works as the placeholder; `_` is conventional.

# Rule: External UID Pattern

Do not create users inside container images. Let the orchestrator (Podman Quadlet, Kubernetes, docker-compose) specify the UID and GID the container runs as.

This keeps images portable across Docker, Podman, and Kubernetes without runtime-specific flags. Kubernetes clusters that enforce Pod Security Standards or OPA Gatekeeper can require arbitrary UIDs; a hardcoded `USER` in the image breaks this. Images also stay smaller (no shadow/passwd utilities) and avoid UID collisions across systems.

## Pattern to avoid

```dockerfile
# BAD: Creates internal user, couples image to a specific UID
RUN groupadd -r myapp && useradd -r -g myapp myapp
USER myapp
```

This may require Podman's `:U` volume flag (not portable to Docker), causes ownership conflicts when the orchestrator specifies a different UID, and complicates debugging.

## Recommended

Use a distroless or minimal base image without `USER`:

```dockerfile
FROM gcr.io/distroless/base-debian12:latest AS final
# Pin to digest for production: FROM gcr.io/distroless/base-debian12@sha256:...
COPY --from=builder /app/binary /usr/bin/myapp
ENTRYPOINT ["/usr/bin/myapp"]
```

For Node.js:

```dockerfile
FROM docker.io/library/node:22-bookworm-slim AS runtime
WORKDIR /app
COPY --from=build /app/dist ./dist
COPY --from=build /app/node_modules ./node_modules
ENTRYPOINT ["node", "dist/index.js"]
```

Without `USER`, containers run as root (UID 0) by default. The orchestrator must set a non-root UID and GID.

## Orchestrator configuration

Podman Quadlet:

```ini
[Container]
User=1100
Group=1100
Volume=/var/lib/myapp:/data:rw
```

Kubernetes (pod-level `securityContext`):

```yaml
securityContext:
  runAsUser: 1100
  runAsGroup: 1100
  runAsNonRoot: true
  fsGroup: 1100
```

docker-compose:

```yaml
services:
  myapp:
    user: "1100:1100"
    volumes:
      - ./data:/data
```

## Ansible host user setup

Create the host user with a deterministic UID matching the orchestrator configuration, then reference it in Quadlet:

```yaml
- name: Create myapp group
  ansible.builtin.group:
    name: myapp
    gid: 1100

- name: Create myapp user
  ansible.builtin.user:
    name: myapp
    uid: 1100
    group: myapp

- name: Create data directory
  ansible.builtin.file:
    path: /var/lib/myapp
    state: directory
    owner: myapp
    group: myapp
    mode: "0750"
```

# Rule: Prefer Debian Slim Over Alpine Base Images

Use `-slim` Debian variants (e.g. `node:22-bookworm-slim`, `python:3.12-slim-bookworm`) as container base images — Alpine's musl libc breaks glibc prebuilt binaries and forces native-module rebuilds, and the base-image size savings vanish once app dependencies land.

# Rule: Prefer OCI Images

Build and distribute container images in OCI format rather than Docker format. OCI is the open industry standard, supported by every modern container tool. The OCI image spec derived from Docker v2 schema 2, but OCI avoids vendor lock-in.

## Building

**Docker Buildx/BuildKit.** Docker Desktop 4.31+ defaults to OCI media types. For older versions, set `oci-mediatypes=true` explicitly:

```bash
docker buildx build \
  --output type=image,name=REG/IMG:TAG,push=true,oci-mediatypes=true \
  .
```

Export as an OCI layout tarball:

```bash
docker buildx build --output type=oci,dest=img.oci.tar .
```

**Podman/Buildah.** OCI is the default. Pass `--format oci` to force it explicitly.

## Verifying

```bash
skopeo inspect --raw docker://REG/IMG:TAG | jq -r .mediaType
```

| Format | Single-arch manifest                                   | Multi-arch index                                            |
| ------ | ------------------------------------------------------ | ----------------------------------------------------------- |
| OCI    | `application/vnd.oci.image.manifest.v1+json`           | `application/vnd.oci.image.index.v1+json`                   |
| Docker | `application/vnd.docker.distribution.manifest.v2+json` | `application/vnd.docker.distribution.manifest.list.v2+json` |

# Rule: Avoid Leaky Abstractions

Design interfaces around what callers need, not how the system works internally. An abstraction is leaky when using it correctly requires knowledge of underlying storage, infrastructure, or error behavior. Keep signatures consistent, return domain types instead of backend artifacts, and inject infrastructure dependencies through constructors rather than method parameters.

## Warning signs

- Inconsistent method signatures that reflect backend differences
- Infrastructure details (connection strings, transaction handles) exposed in the interface
- Large performance differences between similar operations
- Errors that force callers to understand underlying layers

## Example

```ts
// Leaky: exposes database concerns, inconsistent signatures
interface ReservationRepository {
  create(restaurantId: number, reservation: Reservation): number; // returns DB ID
  findById(id: string): Reservation | null; // why no restaurantId?
  update(reservation: Reservation): void;
  connect(connectionString: string): void;
}

// Better: consistent interface, infrastructure hidden, injected via constructor
interface ReservationRepository {
  create(restaurantId: number, draft: NewReservation): Promise<Reservation>;
  findById(restaurantId: number, id: string): Promise<Reservation | null>;
  update(restaurantId: number, reservation: Reservation): Promise<void>;
}
```

# Rule: Comments Explain Why, Not What

Default to writing no comments. Only add one when the WHY is non-obvious — a hidden constraint, a subtle invariant, a workaround for a specific bug, behavior that would surprise a reader. If removing the comment wouldn't confuse a future reader, don't write it.

When a comment is warranted, capture intent, constraints, and reasoning the code cannot show: why a decision was made, which alternatives were rejected, what external factor forced a workaround. That's what future readers cannot recover from the code alone, and it stops the next person from "cleaning up" something load-bearing.

Never explain WHAT the code does. Names convey purpose, types convey shape, the code itself conveys behavior. Never reference the current task, fix, or callers ("used by X", "added for the Y flow", "handles the case from issue #123") — those belong in the PR description and rot as the codebase evolves. Don't add comments, docstrings, or type annotations to code you didn't change.

Keep comments to one short line. Never write multi-paragraph docstrings or multi-line comment blocks.

```ts
// BAD: restates what the code says
// Increment counter by 1
counter += 1;

// BAD: references caller context that will rot
// Used by the checkout flow after the Stripe webhook fires
function markOrderPaid(orderId: string) {
  /* ... */
}

// GOOD: records a non-obvious external constraint
// Stripe rejects descriptions over 500 chars; truncate defensively
const description = raw.slice(0, 500);
```

# Rule: Early Returns

Handle edge cases and invalid states at the top of a function with guard clauses that return early. Invert conditions and exit immediately: null checks, permission checks, validation, empty collections. Main logic stays at the top level with minimal indentation.

# Rule: File Naming Matches Contents

Name files for what the module does. Use kebab-case and prefer verb-noun or domain-role names. Match the primary export; if you cannot name it crisply, split the file.

## Checklist

- Match the main export: `calculateUsageRate` goes in `calculate-usage-rate.ts`.
- One responsibility per file; if you need two verbs, split it.
- Align with functional core/imperative shell conventions:
  - Functional core: `calculate-…`, `validate-…`, `parse-…`, `format-…`, `aggregate-…`
  - Imperative shell: `…-route.ts`, `…-handler.ts`, `…-job.ts`, `…-cli.ts`, `…-script.ts`
- Prefer specific domain nouns; avoid generic buckets like `utils`, `helpers`, `core`, `data`, `math`.
- Use role suffixes (`-service`, `-repository`) only when they clarify architecture.

Example: A file named `usage.core.ts` containing both fetching and aggregation logic should be split into `fetch-service-usage.ts` and `aggregate-usage.ts`.

# Rule: Functional Core, Imperative Shell

Separate business logic from side effects by organizing code into a functional core and an imperative shell. The functional core contains pure functions that operate only on provided data, free of I/O, database calls, or state mutations. The imperative shell handles all side effects and orchestrates the core to perform work.

This separation improves testability (core logic tests need no mocks), maintainability (shell can change without touching business rules), and reusability (core functions work in any context).

**Functional core:** filtering, mapping, calculations, validation, parsing, formatting, business rule evaluation.

**Imperative shell:** HTTP handlers, database queries, file I/O, API calls, message queue operations, CLI entry points.

```ts
// Bad: Logic and side effects mixed
function sendUserExpiryEmail(): void {
  for (const user of db.getUsers()) {
    if (user.subscriptionEndDate > new Date()) continue;
    if (user.isFreeTrial) continue;
    email.send(user.email, `Your account has expired ${user.name}.`);
  }
}

// Good: Functional core (pure, testable)
function getExpiredUsers(users: User[], cutoff: Date): User[] {
  return users.filter((user) => user.subscriptionEndDate <= cutoff && !user.isFreeTrial);
}

function generateExpiryEmails(users: User[]): Array<[string, string]> {
  return users.map((user) => [user.email, `Your account has expired ${user.name}.`]);
}

// Imperative shell (orchestrates side effects)
email.bulkSend(generateExpiryEmails(getExpiredUsers(db.getUsers(), new Date())));
```

Test the functional core, not the shell. Core tests are fast, deterministic, and need no mocks; the shell becomes thin orchestration where bugs are easy to spot through review. If shell tests are explicitly requested, prefer integration tests over unit tests with mocks.

# Rule: No Logic in Tests

Write test assertions as concrete input/output examples, not computed values. Avoid operators, string concatenation, loops, and conditionals in test bodies—these obscure bugs and make tests harder to verify at a glance.

```ts
const baseUrl = "http://example.com/";

// Bad: computed expectation hides bugs when test and production share the same error
expect(getPhotosUrl()).toBe(baseUrl + "/photos"); // passes despite double-slash bug

// Good: literal expected value catches the bug immediately
expect(getPhotosUrl()).toBe("http://example.com/photos"); // fails, reveals the issue
```

Unlike production code that handles varied inputs, tests verify specific cases. State expectations directly rather than computing them. When a test fails, the expected value should be immediately readable without mental evaluation.

Use test utilities for setup and data preparation—fixtures, builders, factories, mock configuration—but never for computing expected values. Keep assertion logic in the test body with literal expectations.

# Rule: Parse, Don't Validate

When checking input data, return a refined type that preserves the knowledge gained—don't just validate and discard. Validation functions that return `void` or throw errors force callers to re-check conditions or handle "impossible" cases. Parsing functions that return more precise types eliminate redundant checks and let the compiler catch inconsistencies.

Zod embodies this principle: every schema is a parser that transforms `unknown` input into a typed output. Use Zod at system boundaries to parse external data into domain types.

```ts
import * as z from "zod";

// Schema defines both validation rules AND the resulting type
const User = z.object({
  id: z.string(),
  email: z.email(),
  roles: z.array(z.string()).min(1),
});

type User = z.infer<typeof User>;

// Parse at the boundary - downstream code receives typed data
function handleRequest(body: unknown): User {
  return User.parse(body); // throws ZodError if invalid
}
```

## Practical guidance

- **Parse at system boundaries.** Convert external input (JSON, environment variables, API responses) to precise domain types early. Use `.parse()` or `.safeParse()`.
- **Strengthen argument types.** Instead of accepting `T | undefined`, require callers to provide already-parsed data.
- **Let schemas encode constraints.** If a function needs a non-empty array, positive number, or valid email, define a schema that encodes that guarantee.
- **Treat `void`-returning checks with suspicion.** A function that validates but returns nothing is easy to forget.
- **Use `.refine()` for custom constraints.** When built-in validators aren't enough, add refinements that preserve type information.

```ts
// Custom constraint with .refine()
const PositiveInt = z
  .number()
  .int()
  .refine((n) => n > 0, "must be positive");
type PositiveInt = z.infer<typeof PositiveInt>;
```

# Rule: Use `repoq` for Repository Queries

Use `repoq` for reading repository state instead of piping `git` or the forge CLI through `awk`/`jq`/`grep`. Each command handles edge cases (detached HEAD, unborn branches, missing auth) and returns validated JSON. Use raw `git` for commit/push/merge, and the repo's forge CLI for forge-side mutations (PRs, issues, releases) — `gh` for GitHub or `fgj` for Forgejo, per the detected provider. Run `npx -y repoq@latest --help` if unsure of the available subcommands; the explicit tag prevents `npx` from reusing a stale cached release.

# Rule: Cargo Dependency Updates

Run `cargo update` to upgrade dependencies to the latest versions allowed by existing SemVer ranges; this modifies `Cargo.lock` only. By default, Cargo treats plain version specifiers (`"1.0"`, `"0.12"`) as caret (`^`) ranges that allow updates up to, but not including, the next SemVer-breaking release.

Edit `Cargo.toml` only to widen the range itself, such as bumping `serde = "1.0"` to `serde = "2.0"` to adopt a new major version.

For `0.x` versions, Cargo treats minor bumps as breaking: `"0.12"` allows updates within `0.12.x` but not to `0.13.0`. Moving from `"0.12"` to `"0.13"` therefore requires a `Cargo.toml` edit, not `cargo update`.
