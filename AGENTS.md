# Rule: Mandatory Startup Reads

Before taking any action, read @README.md for project overview and context. If it does not exist, skip silently and continue.

# Rule: `askpplx` CLI Usage

Use `askpplx` for real-time web search via Perplexity. Verify external facts—documentation, API behavior, library versions, best practices—before acting on them. A lookup costs far less than debugging hallucinated code. Run `npx -y askpplx --help` if unsure of the available options.

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

Handle edge cases and invalid states at the top of a function with guard clauses that return early. Invert conditions and exit immediately—null checks, permission checks, validation, empty collections. Main logic stays at the top level with minimal indentation.

# Rule: File Naming Matches Contents

Name files for what the module actually does. Use kebab-case and prefer verb-noun or domain-role names. Match the primary export; if you cannot name it crisply, split the file.

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
  return users.filter(
    (user) => user.subscriptionEndDate <= cutoff && !user.isFreeTrial,
  );
}

function generateExpiryEmails(users: User[]): Array<[string, string]> {
  return users.map((user) => [
    user.email,
    `Your account has expired ${user.name}.`,
  ]);
}

// Imperative shell (orchestrates side effects)
email.bulkSend(
  generateExpiryEmails(getExpiredUsers(db.getUsers(), new Date())),
);
```

Test the functional core, not the shell. Core tests are fast, deterministic, and need no mocks; the shell becomes thin orchestration where bugs are easy to spot through review. If shell tests are explicitly requested, prefer integration tests over unit tests with mocks.

# Rule: Inline Obvious Code

Keep simple, self-explanatory code inline rather than extracting it into functions. Every abstraction carries cognitive cost—readers must jump to another location, parse a signature, and track context. For obvious logic, this overhead exceeds any benefit.

Extracting code into a function is not inherently virtuous. A function should exist because it encapsulates meaningful complexity, not because code appears twice.

```ts
// GOOD: Inline obvious logic
if (removedFrom.length === 0) {
  return { ok: true, message: "No credentials found" };
}
return { ok: true, message: `Removed from ${removedFrom.join(" and ")}` };

// BAD: Extraction hides obvious logic behind indirection
return formatRemovalResult(removedFrom);
```

## When to extract

Extract when duplication causes real maintenance risk, not merely because code appears twice:

- A name clarifies complex intent
- Multiple call sites must stay in lockstep and silent divergence would be a bug
- The function encapsulates a coherent standalone concept
- Testing it in isolation provides value

Don't extract for hypothetical reuse:

- For a single caller
- Because "we might need this elsewhere"
- When the name describes implementation rather than purpose

## The wrong abstraction

Abstractions decay when requirements diverge: programmer A extracts duplication into a shared function, programmer B adds a parameter for different behavior, and this repeats until the "abstraction" is a mess of conditionals. When an abstraction proves wrong, re-introduce duplication and let the code show you what's actually shared. Duplication is far cheaper than the wrong abstraction.

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

# Rule: Package Manager Execution

How different package manager commands resolve binaries:

| Command           | Behavior                                                                |
| ----------------- | ----------------------------------------------------------------------- |
| `pnpm exec foo`   | Runs from `./node_modules/.bin`; falls back to system PATH              |
| `pnpx foo`        | Always fetches from registry (uses dlx cache); ignores local installs   |
| `npx foo`         | Checks local `node_modules/.bin` → global → downloads from registry     |
| `npx foo@version` | Resolves version, uses local if exact match exists, otherwise downloads |

`pnpx` is an alias for `pnpm dlx`.

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

Use `repoq` for reading repository state instead of piping `git`/`gh` through `awk`/`jq`/`grep`. Each command handles edge cases (detached HEAD, unborn branches, missing auth) and returns validated JSON. Use raw `git`/`gh` for mutations (commit, push, merge). Run `npx -y repoq --help` if unsure of the available subcommands.

# Rule: Cargo Dependency Updates

Run `cargo update` to upgrade dependencies to the latest versions allowed by existing SemVer ranges; this modifies `Cargo.lock` only. By default, Cargo treats plain version specifiers (`"1.0"`, `"0.12"`) as caret (`^`) ranges that allow updates up to, but not including, the next SemVer-breaking release.

Edit `Cargo.toml` only to widen the range itself, such as bumping `serde = "1.0"` to `serde = "2.0"` to adopt a new major version.

For `0.x` versions, Cargo treats minor bumps as breaking: `"0.12"` allows updates within `0.12.x` but not to `0.13.0`. Moving from `"0.12"` to `"0.13"` therefore requires a `Cargo.toml` edit, not `cargo update`.
