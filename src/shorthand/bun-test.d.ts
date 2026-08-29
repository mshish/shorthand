/**
 * Fork-only. Minimal ambient types for Bun-specific APIs, scoped to what
 * `branding.test.ts` actually calls: the `bun:test` module, and the
 * `import.meta.dir` runtime extension it uses to locate fixture files.
 *
 * There is no `bun-types`/`@types/bun` devDependency in this repo — adding
 * one would violate this plan's "zero new dependencies" rule (see
 * docs/superpowers/plans/2026-08-26-fork-only-translation-catalogues.md,
 * Global Constraints), which exists to keep upstream's package.json/bun.lock
 * free of permanent merge-conflict surface. `bun test` itself resolves both
 * `bun:test` and `import.meta.dir` at runtime with no help from this file;
 * this shim exists only so `tsc` (part of `bun run build`) can typecheck them
 * too. Widen it if a later test needs a matcher, helper, or Bun global not
 * listed here.
 */
declare module "bun:test" {
  export function describe(name: string, fn: () => void): void;
  export function test(name: string, fn: () => void | Promise<void>): void;
  export function expect<T>(actual: T): {
    toBe(expected: T): void;
    toEqual(expected: T): void;
    toMatch(expected: RegExp | string): void;
    not: {
      toBe(expected: T): void;
      toEqual(expected: T): void;
      toMatch(expected: RegExp | string): void;
    };
  };
}

interface ImportMeta {
  /** Absolute path to the directory containing the current module. */
  readonly dir: string;
}
