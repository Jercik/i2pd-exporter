# Rule: Mandatory Startup Reads

Before taking any action—answering questions, editing files, or running commands—read these files in order:

- **@README.md** — Project overview and context

If a file does not exist, skip it silently and continue.

# Rule: `askpplx` CLI Usage

**At session start:** Run `npx -y askpplx --help` to confirm the tool works and learn available options.

Use `askpplx` to query Perplexity for real-time web search. Use it to verify external facts before acting—documentation, API behavior, library versions, best practices. A lookup is far cheaper than debugging hallucinated code or explaining why an approach failed. Verification is fast and cheap—prefer looking up information over making assumptions. When in doubt, verify.

# Rule: Avoid Leaky Abstractions

Design abstractions around consumer needs, not implementation details. A leaky abstraction forces callers to understand the underlying system to use it correctly—defeating its purpose. While all non-trivial abstractions leak somewhat (Joel Spolsky's Law of Leaky Abstractions), minimize leakage by ensuring your interface doesn't expose internal constraints, infrastructure artifacts, or inconsistent behavior.

## Warning signs

- **Inconsistent signatures**: Some methods require parameters others don't, revealing backend differences
- **Infrastructure artifacts**: Connection strings, database IDs, or ORM-specific constructs in the API
- **Performance surprises**: Logically equivalent operations with vastly different performance
- **Implementation-dependent error handling**: Callers must catch specific exceptions from underlying layers
- **Required internal knowledge**: Using the abstraction safely requires understanding what's beneath it

## Example

```ts
// Leaky: exposes database concerns, inconsistent signatures
interface ReservationRepository {
  create(restaurantId: number, reservation: Reservation): number; // returns DB ID
  findById(id: string): Reservation | null; // why no restaurantId here?
  update(reservation: Reservation): void;
  connect(connectionString: string): void;
  disconnect(): void;
}

// Better: consistent interface, infrastructure hidden
interface ReservationRepository {
  create(restaurantId: number, reservation: Reservation): Promise<void>;
  findById(restaurantId: number, id: string): Promise<Reservation | null>;
  update(restaurantId: number, reservation: Reservation): Promise<void>;
}

// Connection management injected, not exposed
class PostgresReservationRepository implements ReservationRepository {
  constructor(private readonly pool: Pool) {}
  // ...
}
```

## Practical guidance

- Design interfaces for what callers need to do, not how you implement it
- Keep signatures consistent—if one method needs context, similar methods should too
- Return domain types, not infrastructure artifacts (avoid raw database IDs)
- Inject infrastructure dependencies through constructors, not method parameters
- Normalize error handling so callers don't catch implementation-specific exceptions

# Rule: Early Returns

Handle edge cases and invalid states at the top of a function with guard clauses that return early. This flattens nested conditionals and keeps the happy path obvious.

```ts
function getDiscount(user: User | null) {
  if (!user) return 0;
  if (!user.isActive) return 0;
  if (user.membership === "premium") return 0.2;
  return 0.1;
}
```

Invert conditions and exit immediately—null checks, permission checks, validation, empty collections. Main logic stays at the top level with minimal indentation.

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

## Testing strategy

Focus testing on the functional core. These tests are fast, deterministic, need no mocks, and provide high value per line of test code. Do not write tests for the imperative shell unless the user explicitly requests them—when the core is well-tested, the shell becomes thin orchestration where bugs are easy to spot through review.

If shell tests are explicitly requested, prefer integration tests over unit tests with mocks.

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

Extract when a name clarifies complex intent, you need consistent behavior across many call sites, the function encapsulates a coherent standalone concept, or testing it in isolation provides value. Don't extract for single callers, because "we might need this elsewhere," or when the name describes implementation rather than purpose.

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

Test utilities are acceptable for setup and data preparation—fixtures, builders, factories, mock configuration—but not for computing expected values. Keep assertion logic in the test body with literal expectations.

# Rule: Normalize User Input

Accept flexible input formats and normalize programmatically. Don't reject input because of formatting characters users naturally include—spaces in credit card numbers, parentheses in phone numbers, hyphens in IDs. Computers are good at removing that.

```ts
import * as z from "zod";

// BAD - forces users to format input a specific way
const phoneSchema = z.string().regex(/^\d{10}$/, "Only digits allowed");

// GOOD - accept flexible input, normalize it
const phoneSchema = z
  .string()
  .transform((s) => s.replace(/[\s().-]/g, ""))
  .pipe(z.string().regex(/^\d{10}$/, "Must be 10 digits"));
```

When accepting user input:

- **Strip formatting characters** (spaces, hyphens, parentheses, dots) before validation
- **Trim whitespace** from text fields
- **Normalize case** when case doesn't matter (emails, usernames)
- **Accept common variations** (with/without country code for phones, with/without protocol for URLs)

**Never normalize passwords.** Users should be able to use any characters exactly as entered—normalizing passwords reduces entropy and can break legitimate credentials. The only acceptable transformation is Unicode normalization (NFC/NFKC) for cross-platform compatibility before hashing.

The validation error should describe what's actually wrong with the data, not complain about formatting the computer could have handled.

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
- **Strengthen argument types.** Instead of returning `T | undefined`, require callers to provide already-parsed data.
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

Run `npx -y repoq --help` to learn available options.

Use `repoq` instead of piping `git`/`gh` commands through `awk`/`jq`/`grep`.
Each command handles edge cases (detached HEAD, unborn branches, missing auth)
and returns validated JSON. Prefer `repoq` for reading state; use raw `git`/`gh`
for mutations (commit, push, merge).

# Rule: Cargo Dependency Updates

Use `cargo update` to upgrade dependencies to the latest versions within existing semver ranges. Version specifiers like `"1.0"` or `"0.12"` use the caret (`^`) operator by default, allowing updates up to (but not including) the next breaking change. Edit `Cargo.toml` only when changing the semver constraint itself, such as upgrading to a new major version (`warp = "0.3"` to `warp = "0.4"`).

Note: For `0.x` versions, Cargo treats minor version bumps as breaking—`"0.12"` allows updates within `0.12.x` but not to `0.13.0`.
