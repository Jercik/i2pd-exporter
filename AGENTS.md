# Rule: `askpplx` CLI Usage

**MANDATORY:** Run `npx -y askpplx --help` at the start of every agent session to learn available options and confirm the tool is working.

Use `askpplx` to query Perplexity, an AI search engine combining real-time web search with advanced language models.

## Why This Matters

- **Ground your knowledge:** Your training data has a cutoff date. Real-time search ensures you work with current information—correct API signatures, latest versions, up-to-date best practices.
- **Save time and resources:** A quick lookup is far cheaper than debugging hallucinated code or explaining why an approach failed. When in doubt, verify first.
- **Reduce false confidence:** Even when you feel certain, external verification catches subtle errors before they compound into larger problems.
- **Stay current:** Libraries change, APIs deprecate, patterns evolve. What was correct six months ago may be wrong today.

## Usage Guidelines

Use concise prompts for quick facts and focused questions for deeper topics. If results are unexpected, refine your query and ask again. Verification is fast and cheap—prefer looking up information over making assumptions.


---

# Rule: Avoid Leaky Abstractions

Design abstractions around consumer needs, not implementation details. A leaky abstraction forces callers to understand the underlying system to use it correctly—defeating its purpose. While all non-trivial abstractions leak somewhat (Joel Spolsky's Law), minimize leakage by ensuring your interface doesn't expose internal constraints, infrastructure artifacts, or inconsistent behavior.

## Warning signs

- **Inconsistent signatures**: Some methods require parameters others don't, revealing backend differences
- **Infrastructure artifacts**: Connection strings, database IDs, or ORM-specific constructs in the API
- **Performance surprises**: Logically equivalent operations with vastly different performance
- **Implementation-dependent error handling**: Callers must catch specific exceptions from underlying layers
- **Required internal knowledge**: Using the abstraction safely requires understanding what's beneath it

## Before/after example

```ts
// Leaky: Exposes database concerns, inconsistent signature
interface ReservationRepository {
  create(restaurantId: number, reservation: Reservation): number; // Returns DB ID
  findById(id: string): Reservation | null; // No restaurantId needed?
  update(reservation: Reservation): void;
  connect(connectionString: string): void;
  disconnect(): void;
}
```

```ts
// Better: Consistent interface, infrastructure hidden
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
- Keep signatures consistent—if one method needs context, all similar methods should accept it
- Return domain types, not infrastructure artifacts (avoid returning raw database IDs)
- Inject infrastructure dependencies through constructors, not method parameters
- Normalize error handling so callers don't need to catch implementation-specific exceptions
- Prefer focused interfaces over "fat" interfaces with unrelated methods


---

# Rule: Early Returns

Use guard clauses to handle edge cases and invalid states at the top of a function, then return early. This flattens nested conditionals, makes the happy path obvious, and reduces cognitive load.

```ts
// Nested (hard to follow)
function getDiscount(user: User | null) {
  if (user) {
    if (user.isActive) {
      if (user.membership === "premium") {
        return 0.2;
      } else {
        return 0.1;
      }
    }
  }
  return 0;
}

// Flat (guard clauses)
function getDiscount(user: User | null) {
  if (!user) return 0;
  if (!user.isActive) return 0;
  if (user.membership === "premium") return 0.2;
  return 0.1;
}
```

Guard clauses invert the condition and exit immediately, leaving the main logic at the top level with minimal indentation. Each guard documents a precondition the function requires.

## When to use

- Null/undefined checks
- Permission or authorization checks
- Validation of required preconditions
- Empty collection checks (`if (items.length === 0) return []`)

## When to reconsider

- **Many guards accumulating**: If you need 5+ guard clauses, the function may have too many responsibilities—consider splitting it
- **Guards with side effects**: Keep guards as pure condition checks; don't mix in logging, mutations, or complex logic
- **Resource cleanup required**: In languages without RAII or `defer`, multiple returns can complicate cleanup (less relevant in JS/TS with garbage collection)

The goal is clarity, not dogma. A single well-placed `if-else` is fine when both branches represent equally valid paths rather than a precondition check.


---

# Rule: File Naming Matches Contents

Name files for what the module actually does. Use kebab-case and prefer verb–noun or domain–role names. Match the primary export; if you can’t name it crisply, split the file.

## Checklist

- Use kebab-case; describe responsibility (verb–noun or domain–role).
- Match the main export: `calculateUsageRate` → `calculate-usage-rate.ts`.
- One responsibility per file; if you need two verbs, split.
- Align with functional core/imperative shell:
  - Functional core: `calculate-…`, `validate-…`, `parse-…`, `format-…`, `aggregate-…`
  - Imperative shell: `…-route.ts`, `…-handler.ts`, `…-job.ts`, `…-cli.ts`, `…-script.ts`
- Prefer specific domain nouns; avoid buckets like `utils`, `helpers`, `core`, `data`, `math`.
- Use role suffixes only when they clarify architecture (e.g., `-service`, `-repository`).
- Preserve history when renaming: `git mv old-name.ts new-name.ts`.

Example: `usage.core.ts` → split into `fetch-service-usage.ts` and `aggregate-usage.ts`.


---

# Rule: Functional Core, Imperative Shell

Separate business logic from side effects by organizing code into a functional core and an imperative shell. The functional core contains pure, testable functions that operate only on provided data, free of I/O operations, database calls, or external state mutations. The imperative shell handles all side effects and orchestrates the functional core to perform business logic.

This separation improves testability, maintainability, and reusability. Core logic can be tested in isolation without mocking external dependencies, and the imperative shell can be modified or swapped without changing business logic.

Example of mixed logic and side effects:

```ts
// Bad: Logic and side effects are mixed
function sendUserExpiryEmail(): void {
  for (const user of db.getUsers()) {
    if (user.subscriptionEndDate > new Date()) continue;
    if (user.isFreeTrial) continue;
    email.send(user.email, "Your account has expired " + user.name + ".");
  }
}
```

Refactored using functional core and imperative shell:

```ts
// Functional core - pure functions with no side effects
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

// Imperative shell - handles side effects
email.bulkSend(
  generateExpiryEmails(getExpiredUsers(db.getUsers(), new Date())),
);
```

The functional core functions can now be easily tested with sample data and reused for different purposes without modification.


---

# Rule: Inline Obvious Code

Keep simple, self-explanatory code inline rather than extracting it into functions. Every abstraction carries cognitive cost—readers must jump to another location, parse a function signature, and mentally track the context switch. For obvious logic, this overhead exceeds any benefit.

> "Functions should be short and sweet, and do just one thing. They should fit on one or two screenfuls of text... and do one thing and do that well."
> — Linux kernel coding style

The key insight: extracting code into a function is not inherently virtuous. A function should exist because it encapsulates meaningful complexity, not because code appears twice.

## When to inline

Inline code when:

- The logic is immediately understandable (a few lines, no complex branching)
- It appears in only one or two places
- Extracting it would require reading the function definition to understand what happens

```ts
// GOOD: Inline obvious logic—instantly readable
if (removedFrom.length === 0) {
  return { ok: true, message: "No credentials found" };
}
return { ok: true, message: `Removed from ${removedFrom.join(" and ")}` };

// BAD: Extraction hides obvious logic behind indirection
return formatRemovalResult(removedFrom);
```

Another example—null checks that don't need abstraction:

```ts
// GOOD: Simple null guard, inline
function enrich(json: JsonObject, data: string | null): JsonObject {
  if (data === null) return json;
  return { ...json, data };
}

// BAD: Over-abstracted null-safe wrapper
const enrichSafe = nullSafe((json, data) => ({ ...json, data }));
```

The second version adds a layer of indirection for a two-line null check. The abstraction costs more to understand than the duplication it eliminates.

## When to extract

Extract into a function when:

- The logic is complex enough that a name genuinely clarifies intent
- You need to enforce consistent behavior across many call sites (not just two or three)
- The function encapsulates a coherent concept that stands alone
- Testing the logic in isolation provides real value
- The number of local variables exceeds what you can track mentally (the Linux kernel uses ~10 as a threshold)

## The wrong abstraction

Sandi Metz observes that abstractions decay when requirements diverge:

1. Programmer A extracts duplication into a shared function
2. Programmer B needs slightly different behavior, adds a parameter and conditional
3. This repeats until the "abstraction" is a mess of parameters and branches

The result is harder to understand than the original duplication. When an abstraction proves wrong, re-introduce duplication and let the code show you what's actually shared.

```ts
// Started as shared abstraction, became a mess
function NavButton({ label, url, icon, highlight, testId, onClick, disabled, badge }) {
  // 50 lines of conditional logic for "shared" button
}

// Better: Accept that these aren't the same thing
<HomeButton />
<AboutButton />
<BuyButton highlight testId="buy-cta" />
```

## Warning signs of bad extraction

- **Conditional parameters**: Passing flags that determine which code path executes
- **Single caller**: A "reusable" function called from exactly one place
- **Name describes implementation**: `formatRemovalResult` vs. a name that describes _why_
- **Reading the function is required**: The call site doesn't make sense without jumping to the definition
- **Future-proofing**: "We might need this elsewhere" without concrete evidence

## The cognitive test

Before extracting, ask: "Will readers understand this faster by reading the inline code or by jumping to a function definition?" If inline is faster, don't extract.

> "Duplication is far cheaper than the wrong abstraction."
> — Sandi Metz

Three similar lines repeated twice cost less mental effort than a helper function that requires a context switch to understand. A single, direct block of code is cognitively cheaper than one fractured into pointless subroutines.


---

# Rule: No Logic in Tests

Write test assertions as concrete input/output examples, not computed values. Avoid operators, string concatenation, loops, and conditionals in test bodies—these obscure bugs and make tests harder to verify at a glance.

```ts
// Bad: this test passes, but both production code and test share the same bug
const baseUrl = "http://example.com/";
function getPhotosUrl() {
  return baseUrl + "/photos"; // Bug: produces "http://example.com//photos"
}
expect(getPhotosUrl()).toBe(baseUrl + "/photos"); // ✓ passes — bug hidden

// Good: literal expected value reveals the bug immediately
expect(getPhotosUrl()).toBe("http://example.com/photos"); // ✗ fails — bug caught
```

Unlike production code that handles varied inputs, tests verify specific cases. State expectations directly rather than computing them. When a test fails, the expected value should be immediately readable without mental evaluation.

If test setup genuinely requires complex logic (fixtures, builders, shared assertions), extract it into dedicated test utilities with their own tests. Keep test bodies simple: arrange inputs, call the function, assert against literal expected values.


---

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

The validation error should describe what's actually wrong with the data, not complain about formatting the computer could have handled.


---

# Rule: Test Functional Core

Focus testing efforts on the functional core—pure functions with no side effects. These tests are fast, deterministic, and provide high value per line of test code. Do not write tests for the imperative shell (I/O, database calls, external services) unless the user explicitly requests them.

Imperative shell tests require mocks, stubs, or integration infrastructure, making them slower to write, brittle to maintain, and harder to debug. The return on investment diminishes rapidly compared to functional core tests. When the functional core is well-tested, the imperative shell becomes thin orchestration code where bugs are easier to spot through review or manual testing.

## What to test by default

- Pure transformation functions (filtering, mapping, calculations)
- Validation and parsing logic
- Business rule implementations
- Data formatting and serialization helpers

## What to skip unless explicitly requested

- HTTP handlers and route definitions
- Database queries and repository methods
- External API clients
- File system operations
- Message queue consumers/producers

If testing imperative shell code is explicitly requested, prefer integration tests over unit tests with mocks—they catch real issues and are less likely to break when implementation details change.


---

# Rule: Use Git Mv

Use `git mv <old> <new>` for renaming or moving tracked files in Git. It stages both deletion and addition in one command, preserves history for `git log --follow`, and is the only reliable method for case-only renames on case-insensitive filesystems (Windows/macOS).

```bash
git mv old-file.js new-file.js              # Simple rename
git mv file.js src/utils/file.js            # Move to directory
git mv readme.md README.md                  # Case-only change
for f in *.test.js; do git mv "$f" tests/; done  # Multiple files (use shell loop)
```


---

# Rule: Cargo Dependency Updates

When updating Rust dependencies, use `cargo update` to upgrade to the latest compatible versions within existing semver ranges in Cargo.toml. Most dependencies use semver ranges like `"1"`, `"0.12"`, or `"1.0"` (equivalent to `^1.0.0`, `^0.12.0`, etc.) that automatically resolve to the highest compatible version when running `cargo update`. Only edit Cargo.toml when you need to change the semver constraint itself, such as upgrading to a new major version (for example, `warp = "0.3"` → `warp = "0.4"`). Running `cargo update` updates Cargo.lock to the newest versions allowed by your current Cargo.toml ranges without requiring any file edits.
